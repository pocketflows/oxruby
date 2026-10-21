//! Port of `lib/rubocop/cop/lint/ordered_magic_comments.rb`.

use crate::cop::mixin::frozen_string_literal::leading_magic_comments;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "The encoding magic comment should precede all other magic comments.";

#[derive(Default)]
pub struct OrderedMagicComments;

impl Cop for OrderedMagicComments {
    fn name(&self) -> &'static str {
        "Lint/OrderedMagicComments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.buffer_source().is_empty() {
            return;
        }
        let (Some(encoding_line), Some(other_magic_comment_line)) = magic_comment_lines(ps) else {
            return;
        };
        if encoding_line < other_magic_comment_line {
            return;
        }
        let range = ps.line_range(encoding_line + 1);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            let range1 = ps.line_range(encoding_line + 1);
            let range2 = ps.line_range(other_magic_comment_line + 1);
            corrector.replace(range1, ps.slice(range2));
            corrector.replace(range2, ps.slice(range1));
        });
    }
}

fn magic_comment_lines(ps: &ProcessedSource<'_>) -> (Option<usize>, Option<usize>) {
    let mut lines = (None, None);
    for (index, comment) in leading_magic_comments(ps).iter().enumerate() {
        if comment.encoding_specified() {
            lines.0 = Some(index);
        } else if comment.valid() {
            lines.1 = Some(index);
        }
        if lines.0.is_some() && lines.1.is_some() {
            return lines;
        }
    }
    lines
}
