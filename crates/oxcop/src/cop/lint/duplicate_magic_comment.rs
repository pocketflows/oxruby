//! Port of `lib/rubocop/cop/lint/duplicate_magic_comment.rb`.

use crate::cop::mixin::frozen_string_literal::leading_magic_comments;
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "Duplicate magic comment detected.";

#[derive(Default)]
pub struct DuplicateMagicComment;

impl Cop for DuplicateMagicComment {
    fn name(&self) -> &'static str {
        "Lint/DuplicateMagicComment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.buffer_source().is_empty() {
            return;
        }
        for comment_lines in magic_comment_lines(ps) {
            if comment_lines.len() <= 1 {
                continue;
            }
            for comment_line in &comment_lines[1..] {
                let range = ps.line_range(comment_line + 1);
                cx.add_offense_with_correction(range, MSG, |corrector| {
                    corrector.remove(range_by_whole_lines(ps, range, true));
                });
            }
        }
    }
}

/// `[encoding_magic_comments, frozen_string_literal_magic_comments]`.
fn magic_comment_lines(ps: &ProcessedSource<'_>) -> [Vec<usize>; 2] {
    let mut comment_lines = [Vec::new(), Vec::new()];
    for (index, magic_comment) in leading_magic_comments(ps).iter().enumerate() {
        if magic_comment.encoding_specified() {
            comment_lines[0].push(index);
        } else if magic_comment.frozen_string_literal_specified() {
            comment_lines[1].push(index);
        }
    }
    comment_lines
}
