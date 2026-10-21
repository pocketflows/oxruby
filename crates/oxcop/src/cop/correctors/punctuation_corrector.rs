//! Port of `lib/rubocop/cop/correctors/punctuation_corrector.rb`.

use crate::cop::Corrector;
use crate::processed_source::Range;

/// `remove_space(corrector, space_before)`.
pub fn remove_space(corrector: &mut Corrector, space_before: Range) {
    corrector.remove(space_before);
}

/// `add_space(corrector, token)`, given the token's range and source.
pub fn add_space(corrector: &mut Corrector, token_pos: Range, token_source: &str) {
    corrector.replace(token_pos, format!("{token_source} "));
}

/// `swap_comma(corrector, range)`: removes a comma, or adds one after the
/// range.
pub fn swap_comma(corrector: &mut Corrector, range: Option<Range>, source: &str) {
    let Some(range) = range else {
        return;
    };
    if &source[range.begin_pos..range.end_pos] == "," {
        corrector.remove(range);
    } else {
        corrector.insert_after(range, ",");
    }
}
