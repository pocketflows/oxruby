//! Port of `lib/rubocop/cop/bundler/gem_filename.rb`.

use std::path::Path;

use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const GEMFILE_FILES: &[&str] = &["Gemfile", "Gemfile.lock"];
const GEMS_RB_FILES: &[&str] = &["gems.rb", "gems.locked"];

#[derive(Default)]
pub struct GemFilename;

impl Cop for GemFilename {
    fn name(&self) -> &'static str {
        "Bundler/GemFilename"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let file_path = cx.processed_source.file_path();
        let basename = Path::new(&file_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let style = style(cx).trim_start_matches(':');
        let gemfile_required = style == "Gemfile";
        let gems_rb_required = style == "gems.rb";
        let basename = basename.as_str();
        if (gemfile_required && GEMFILE_FILES.contains(&basename))
            || (gems_rb_required && GEMS_RB_FILES.contains(&basename))
        {
            return;
        }
        if gemfile_required && GEMS_RB_FILES.contains(&basename) {
            let message = if basename == "gems.rb" {
                format!("`gems.rb` file was found but `Gemfile` is required (file path: {file_path}).")
            } else {
                format!(
                    "Expected a `Gemfile.lock` with `Gemfile` but found `gems.locked` file \
                     (file path: {file_path})."
                )
            };
            cx.add_global_offense(&message);
        }
        if gems_rb_required && GEMFILE_FILES.contains(&basename) {
            let message = if basename == "Gemfile" {
                format!("`Gemfile` was found but `gems.rb` file is required (file path: {file_path}).")
            } else {
                format!(
                    "Expected a `gems.locked` file with `gems.rb` but found `Gemfile.lock` \
                     (file path: {file_path})."
                )
            };
            cx.add_global_offense(&message);
        }
    }
}
