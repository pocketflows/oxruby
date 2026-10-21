//! Port of `lib/rubocop/cop/mixin/allowed_pattern.rb`.

use serde_yaml_ng::Value;

use crate::cop::Investigation;
use crate::cop::mixin::ruby_regex;

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
/// keep their flags.
pub fn allowed_pattern_regexes(cx: &Investigation<'_, '_>) -> Vec<onig::Regex> {
    ["AllowedPatterns", "IgnoredPatterns"]
        .iter()
        .filter_map(|key| cx.cop_config_value(key))
        .flat_map(|value| match value {
            Value::Sequence(items) => items.iter().collect::<Vec<_>>(),
            other => vec![other],
        })
        .filter_map(config_regex)
        .collect()
}

/// A config value as a regexp: a string source, or a tagged Ruby regexp.
pub fn config_regex(value: &Value) -> Option<onig::Regex> {
    match value {
        Value::String(s) => ruby_regex(s),
        Value::Tagged(tagged) if tagged.tag == "ruby/regexp" => {
            let literal = tagged.value.as_str()?;
            let body = literal.strip_prefix('/')?;
            let end = body.rfind('/')?;
            let (source, flags) = (&body[..end], &body[end + 1..]);
            let mut options = onig::RegexOptions::REGEX_OPTION_NONE;
            if flags.contains('i') {
                options |= onig::RegexOptions::REGEX_OPTION_IGNORECASE;
            }
            if flags.contains('x') {
                options |= onig::RegexOptions::REGEX_OPTION_EXTEND;
            }
            if flags.contains('m') {
                options |= onig::RegexOptions::REGEX_OPTION_MULTILINE;
            }
            onig::Regex::with_options(source, options, onig::Syntax::ruby()).ok()
        }
        _ => None,
    }
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
