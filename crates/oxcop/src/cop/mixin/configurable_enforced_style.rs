//! Port of `lib/rubocop/cop/mixin/configurable_enforced_style.rb`. The
//! style-detection bookkeeping only feeds `--auto-gen-config`, so it is not
//! ported.

use crate::cop::Investigation;
use crate::cop::mixin::util::to_supported_styles;

/// `style` for the `EnforcedStyle` parameter.
pub fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    style_for(cx, "EnforcedStyle")
}

/// `style` for another style parameter name.
/// Ruby's `to_sym` makes a symbol value (`:annotated`, as a spec's config
/// is dumped to YAML) the same as its string.
pub fn style_for<'a>(cx: &Investigation<'a, '_>, parameter: &str) -> &'a str {
    let value = cx.cop_config_str(parameter).unwrap_or("");
    value.strip_prefix(':').unwrap_or(value)
}

/// `supported_styles`.
pub fn supported_styles<'a>(cx: &Investigation<'a, '_>, parameter: &str) -> Vec<&'a str> {
    cx.cop_config_list(&to_supported_styles(parameter))
}

/// `alternative_styles`.
pub fn alternative_styles<'a>(cx: &Investigation<'a, '_>, parameter: &str) -> Vec<&'a str> {
    let style = style_for(cx, parameter);
    supported_styles(cx, parameter)
        .into_iter()
        .filter(|s| *s != style)
        .collect()
}

/// `alternative_style` (exactly two supported styles).
pub fn alternative_style<'a>(cx: &Investigation<'a, '_>, parameter: &str) -> &'a str {
    alternative_styles(cx, parameter)
        .first()
        .copied()
        .unwrap_or("")
}
