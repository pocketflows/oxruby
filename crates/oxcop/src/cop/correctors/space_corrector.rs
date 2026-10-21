//! Port of `lib/rubocop/cop/correctors/space_corrector.rb`.

use crate::ast::token::Token;
use crate::cop::Corrector;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::surrounding_space::{
    Side, offending_empty_no_space, offending_empty_space, side_space_range,
};
use crate::processed_source::ProcessedSource;

/// `SpaceCorrector.empty_corrections`.
pub fn empty_corrections(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    empty_config: Option<&str>,
    left_token: &Token,
    right_token: &Token,
) {
    let range = range_between(left_token.end_pos(), right_token.begin_pos());
    if offending_empty_space(ps, empty_config, left_token, right_token) {
        corrector.remove(range);
        corrector.insert_after(left_token.range, " ");
    } else if offending_empty_no_space(empty_config, left_token, right_token) {
        corrector.remove(range);
    }
}

/// `SpaceCorrector.remove_space`.
pub fn remove_space(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    left_token: &Token,
    right_token: &Token,
) {
    if left_token.is_space_after(ps) {
        corrector.remove(side_space_range(ps, left_token.range, Side::Right, false));
    }
    if !right_token.is_space_before(ps) {
        return;
    }
    corrector.remove(side_space_range(ps, right_token.range, Side::Left, false));
}

/// `SpaceCorrector.add_space`.
pub fn add_space(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    left_token: &Token,
    right_token: &Token,
) {
    if !left_token.is_space_after(ps) {
        corrector.insert_after(left_token.range, " ");
    }
    if right_token.is_space_before(ps) {
        return;
    }
    corrector.insert_before(right_token.range, " ");
}
