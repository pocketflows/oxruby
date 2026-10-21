//! Port of `lib/rubocop/cop/layout/initial_indentation.rb`.

use crate::cop::mixin::range_help::{
    SurroundingSpace, Side, range_between, range_with_surrounding_space,
};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Indentation of first line in file detected.";

#[derive(Default)]
pub struct InitialIndentation;

impl Cop for InitialIndentation {
    fn name(&self) -> &'static str {
        "Layout/InitialIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let Some(first_token) = ps.tokens().iter().find(|t| !t.text.starts_with('#')) else {
            return;
        };
        if first_token.column(ps) == 0 {
            return;
        }
        let pos = first_token.range;
        let space_range = range_with_surrounding_space(
            ps.buffer_source(),
            pos,
            SurroundingSpace {
                side: Side::Left,
                newlines: false,
                ..SurroundingSpace::default()
            },
        );
        if space_range == pos {
            return;
        }
        let space = range_between(space_range.begin_pos, pos.begin_pos);
        cx.add_offense_with_correction(pos, MSG, |corrector| corrector.remove(space));
    }
}
