//! Port of `lib/rubocop/cop/lint/script_permission.rb`.
//!
//! `File.stat(path).executable?` is read as the owner's execute bit: the
//! effective user id isn't available without FFI, and the files checked
//! are normally the user's own.

use crate::cop::{Cop, Investigation};

const SHEBANG: &str = "#!";

#[derive(Default)]
pub struct ScriptPermission;

impl Cop for ScriptPermission {
    fn name(&self) -> &'static str {
        "Lint/ScriptPermission"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        if cx.stdin() {
            return;
        }
        if cfg!(windows) {
            return;
        }
        let ps = cx.processed_source;
        if !ps.raw_source().starts_with(SHEBANG) {
            return;
        }
        if executable(ps.path()) {
            return;
        }
        let Some(comment) = ps.comments().first() else {
            return;
        };
        let message = format_message_from(ps.path());
        let path = ps.path().to_path_buf();
        cx.add_offense_yield(comment.range, &message, |cx| {
            if cx.autocorrect() {
                autocorrect(&path);
            }
        });
    }
}

/// `FileUtils.chmod('+x', processed_source.file_path)`.
fn autocorrect(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(permissions.mode() | 0o111);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// `executable?(processed_source)`: virtual sources (no file on disk) count
/// as executable.
fn executable(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return true;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o100 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        true
    }
}

/// `format_message_from(processed_source)`.
fn format_message_from(path: &std::path::Path) -> String {
    let basename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("Script file {basename} doesn't have execute permission.")
}
