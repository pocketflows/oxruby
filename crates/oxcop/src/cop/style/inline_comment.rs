//! Port of `lib/rubocop/cop/style/inline_comment.rb`.

use crate::cop::mixin::util::comment_line;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Avoid trailing inline comments.";

#[derive(Default)]
pub struct InlineComment;

impl Cop for InlineComment {
    fn name(&self) -> &'static str {
        "Style/InlineComment"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let ranges: Vec<_> = ps
            .comments()
            .iter()
            .filter(|comment| {
                let line = ps.line_of(comment.range.begin_pos);
                !(comment_line(ps.lines()[line - 1])
                    || regex!(r"\A# rubocop:(enable|disable|todo)")
                        .find(comment.text)
                        .is_some())
            })
            .map(|comment| comment.range)
            .collect();
        for range in ranges {
            cx.add_offense(range, MSG);
        }
    }
}
