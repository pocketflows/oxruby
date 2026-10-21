//! Port of `lib/rubocop/project_index_loader.rb`.
//!
//! RuboCop loads the optional `rubydex` gem; oxcop links the `rubydex`
//! crate (what the gem wraps), so the index is always available and
//! `warn_unavailable` never applies.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::project_index::ProjectIndex;

/// `ProjectIndexLoader.build_index(paths)`: the resolved index, or `None`
/// (with a warning) when building it fails.
pub fn build_index(paths: &[PathBuf]) -> Option<Arc<ProjectIndex>> {
    guarded(|| ProjectIndex::from_paths(paths))
}

/// The spec shim's `build_index(uri => source)` (`--project-index-sources`).
pub fn build_index_from_sources(sources: &[(String, String)]) -> Option<Arc<ProjectIndex>> {
    guarded(|| ProjectIndex::from_sources(sources))
}

/// `rescue StandardError => e`: a panic inside rubydex.
fn guarded(build: impl FnOnce() -> ProjectIndex) -> Option<Arc<ProjectIndex>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)) {
        Ok(index) => Some(Arc::new(index)),
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            crate::cli::warn(&format!(
                "rubydex index build failed: {message}. Continuing without it."
            ));
            None
        }
    }
}

/// `ProjectIndexLoader.bundled_gem_source_files`: `lib/**/*.rb` of every
/// gem in the bundle (`gem_dirs`), or nothing outside Bundler.
pub fn bundled_gem_source_files(gem_dirs: Option<Vec<PathBuf>>) -> Vec<PathBuf> {
    let Some(gem_dirs) = gem_dirs else {
        return Vec::new();
    };
    gem_dirs
        .iter()
        .flat_map(|dir| glob_ruby_files(&dir.join("lib")))
        .collect()
}

/// `Dir.glob(File.join(dir, '**', '*.rb'))`: hidden entries are skipped and
/// symlinked directories aren't followed, as Ruby's `**` does.
fn glob_ruby_files(dir: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0 || !entry.file_name().to_string_lossy().starts_with('.')
        })
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.depth() > 0
                && !entry.file_type().is_dir()
                && entry.path().extension().is_some_and(|ext| ext == "rb")
                && (entry.file_type().is_file() || entry.path().is_file())
        })
        .map(walkdir::DirEntry::into_path)
        .collect()
}
