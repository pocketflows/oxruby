//! Port of `lib/rubocop/cop/layout/space_before_comment.rb`.

use crate::cop::{Cop, Investigation};

const MSG: &str = "Put a space before an end-of-line comment.";

#[derive(Default)]
pub struct SpaceBeforeComment;

impl Cop for SpaceBeforeComment {
    fn name(&self) -> &'static str {
        "Layout/SpaceBeforeComment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        for pair in ps.sorted_tokens().windows(2) {
            let (token1, token2) = (&pair[0], &pair[1]);
            if !token2.is_comment() {
                continue;
            }
            if token1.line(ps) != token2.line(ps) {
                continue;
            }
            if token1.end_pos() != token2.begin_pos() {
                continue;
            }
            let range = token2.range;
            cx.add_offense_with_correction(range, MSG, |corrector| {
                corrector.insert_before(range, " ");
            });
        }
    }
}
