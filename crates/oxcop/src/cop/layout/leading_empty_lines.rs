//! Port of `lib/rubocop/cop/layout/leading_empty_lines.rb`.

use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Unnecessary blank line at the beginning of the source.";

#[derive(Default)]
pub struct LeadingEmptyLines;

impl Cop for LeadingEmptyLines {
    fn name(&self) -> &'static str {
        "Layout/LeadingEmptyLines"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let Some(token) = ps.tokens().first() else {
            return;
        };
        if token.line(ps) <= 1 {
            return;
        }
        let range = Range::new(0, token.begin_pos());
        cx.add_offense_with_correction(token.range, MSG, |corrector| {
            corrector.remove(range);
        });
    }
}
