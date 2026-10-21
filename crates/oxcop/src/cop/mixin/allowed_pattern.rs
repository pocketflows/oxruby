//! Port of `lib/rubocop/cop/mixin/allowed_pattern.rb`.

use crate::cop::Investigation;
use crate::cop::mixin::ruby_regex;

/// `allowed_patterns`: `AllowedPatterns` plus the deprecated
/// `IgnoredPatterns`.
pub fn allowed_patterns<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    let mut patterns = cx.cop_config_list("AllowedPatterns");
    patterns.extend(cx.cop_config_list("IgnoredPatterns"));
    patterns
}

/// `matches_allowed_pattern?(line)`.
pub fn matches_allowed_pattern(cx: &Investigation<'_, '_>, line: &str) -> bool {
    allowed_patterns(cx)
        .iter()
        .filter_map(|p| ruby_regex(p))
        .any(|re| re.find(line).is_some())
}

/// `allowed_line?(line)` for a source line.
pub fn allowed_line(cx: &Investigation<'_, '_>, line: &str) -> bool {
    matches_allowed_pattern(cx, line)
}
