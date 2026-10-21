//! Port of `lib/rubocop/cop/mixin/match_range.rb`.

use crate::cop::mixin::range_help::range_between;
use crate::processed_source::Range;

/// `each_match_range(range, regex)`: the range of group 1 of each match of
/// `regex` in `source` (the source of `range`).
pub fn each_match_range(range: Range, source: &str, regex: &onig::Regex) -> Vec<Range> {
    regex
        .captures_iter(source)
        .filter_map(|captures| captures.pos(1))
        .map(|(begin, end)| match_range(range, begin, end))
        .collect()
}

/// `match_range(range, match)`.
pub fn match_range(range: Range, begin: usize, end: usize) -> Range {
    range_between(range.begin_pos + begin, range.begin_pos + end)
}
