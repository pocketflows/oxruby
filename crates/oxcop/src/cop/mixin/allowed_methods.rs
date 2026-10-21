//! Port of `lib/rubocop/cop/mixin/allowed_methods.rb`.

use serde_yaml_ng::Value;

use crate::cop::Investigation;

/// `allowed_methods`: `AllowedMethods` plus the deprecated `IgnoredMethods`
/// and `ExcludedMethods` (unless those contain regexps, which
/// `AllowedPattern` uses instead).
pub fn allowed_methods<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    let mut methods = cx.cop_config_list("AllowedMethods");
    if !deprecated_values_have_regexp(cx) {
        methods.extend(cx.cop_config_list("IgnoredMethods"));
        methods.extend(cx.cop_config_list("ExcludedMethods"));
    }
    methods
}

/// `cop_config_deprecated_values.any?(Regexp)`.
pub fn deprecated_values_have_regexp(cx: &Investigation<'_, '_>) -> bool {
    ["IgnoredMethods", "ExcludedMethods"].iter().any(|key| {
        config_values(cx, key)
            .iter()
            .any(|v| crate::cop::mixin::regexp_literal(v).is_some())
    })
}

/// `Array(cop_config.fetch(key, []))`.
pub fn config_values<'a>(cx: &Investigation<'a, '_>, key: &str) -> Vec<&'a Value> {
    match cx.cop_config_value(key) {
        Some(Value::Sequence(items)) => items.iter().collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(other) => vec![other],
    }
}

/// `allowed_method?(name)`.
pub fn allowed_method(cx: &Investigation<'_, '_>, name: &str) -> bool {
    allowed_methods(cx).contains(&name)
}
