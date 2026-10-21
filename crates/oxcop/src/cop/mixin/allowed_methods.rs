//! Port of `lib/rubocop/cop/mixin/allowed_methods.rb`.

use crate::cop::Investigation;

/// `allowed_methods`: `AllowedMethods` plus the deprecated `IgnoredMethods`
/// and `ExcludedMethods` (unless those are regexps).
pub fn allowed_methods<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    let mut methods = cx.cop_config_list("AllowedMethods");
    methods.extend(cx.cop_config_list("IgnoredMethods"));
    methods.extend(cx.cop_config_list("ExcludedMethods"));
    methods
}

/// `allowed_method?(name)`.
pub fn allowed_method(cx: &Investigation<'_, '_>, name: &str) -> bool {
    allowed_methods(cx).contains(&name)
}
