//! Port of `lib/rubocop/cop/lint/empty_file.rb`.

use crate::cop::mixin::util::comment_line;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Empty file detected.";

#[derive(Default)]
pub struct EmptyFile;

impl Cop for EmptyFile {
    fn name(&self) -> &'static str {
        "Lint/EmptyFile"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        if offending(cx) {
            cx.add_global_offense(MSG);
        }
    }
}

fn offending(cx: &Investigation<'_, '_>) -> bool {
    empty_file(cx) || (!cx.cop_config_bool("AllowComments", false) && contains_only_comments(cx))
}

fn empty_file(cx: &Investigation<'_, '_>) -> bool {
    cx.processed_source.buffer_source().is_empty()
}

fn contains_only_comments(cx: &Investigation<'_, '_>) -> bool {
    cx.processed_source
        .lines()
        .iter()
        .all(|line| line.trim().is_empty() || comment_line(line))
}
