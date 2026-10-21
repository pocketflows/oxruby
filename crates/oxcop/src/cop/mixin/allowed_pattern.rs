//! Port of `lib/rubocop/cop/mixin/allowed_pattern.rb`.

use serde_yaml_ng::Value;

use crate::cop::Investigation;
use crate::cop::mixin::allowed_methods::{config_values, deprecated_values_have_regexp};
use crate::cop::mixin::config_regexp;

/// `allowed_patterns`: `AllowedPatterns` plus the deprecated
/// `IgnoredPatterns`, as strings (regexp values are left out; see
/// [`allowed_pattern_regexes`]).
pub fn allowed_patterns<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    let mut patterns = cx.cop_config_list("AllowedPatterns");
    patterns.extend(cx.cop_config_list("IgnoredPatterns"));
    patterns
}

/// `allowed_patterns` compiled as `Regexp.new(pattern)` does: strings are
/// regexp sources, and Ruby regexps (`!ruby/regexp /source/flags` in YAML)
/// keep their flags. When the deprecated `IgnoredMethods`/`ExcludedMethods`
/// hold a `Regexp`, their values are patterns too.
pub fn allowed_pattern_regexes(cx: &Investigation<'_, '_>) -> Vec<onig::Regex> {
    let mut values = config_values(cx, "AllowedPatterns");
    values.extend(config_values(cx, "IgnoredPatterns"));
    if deprecated_values_have_regexp(cx) {
        values.extend(config_values(cx, "IgnoredMethods"));
        values.extend(config_values(cx, "ExcludedMethods"));
    }
    values.into_iter().filter_map(config_regex).collect()
}

/// A config value as a regexp: a string source, or a tagged Ruby regexp.
pub fn config_regex(value: &Value) -> Option<onig::Regex> {
    config_regexp(value)
}

/// `matches_allowed_pattern?(line)`.
pub fn matches_allowed_pattern(cx: &Investigation<'_, '_>, line: &str) -> bool {
    allowed_pattern_regexes(cx)
        .iter()
        .any(|re| re.find(line).is_some())
}

/// `allowed_line?(line)` for a source line.
pub fn allowed_line(cx: &Investigation<'_, '_>, line: &str) -> bool {
    matches_allowed_pattern(cx, line)
}
