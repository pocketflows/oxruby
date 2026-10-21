//! Port of `lib/rubocop/cop/mixin/configurable_enforced_style.rb`.
//!
//! The style detection (`correct_style_detected` and friends) fills
//! `cx.auto_gen` (`config_to_allow_offenses`, `detected_style`), which feeds
//! `--auto-gen-config` upstream and `cop.config_to_allow_offenses` in specs.
//! Each function takes the style parameter name; `"EnforcedStyle"` is the
//! default `style_parameter_name`.

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

/// `opposite_style_detected`.
pub fn opposite_style_detected(cx: &mut Investigation<'_, '_>, parameter: &str) {
    let alternative = alternative_style(cx, parameter).to_owned();
    style_detected(cx, parameter, &[alternative.as_str()]);
}

/// `correct_style_detected`.
pub fn correct_style_detected(cx: &mut Investigation<'_, '_>, parameter: &str) {
    let style = style_for(cx, parameter).to_owned();
    style_detected(cx, parameter, &[style.as_str()]);
}

/// `unexpected_style_detected(unexpected)`.
pub fn unexpected_style_detected(
    cx: &mut Investigation<'_, '_>,
    parameter: &str,
    unexpected: &str,
) {
    style_detected(cx, parameter, &[unexpected]);
}

/// `ambiguous_style_detected(*possibilities)`.
pub fn ambiguous_style_detected(
    cx: &mut Investigation<'_, '_>,
    parameter: &str,
    possibilities: &[&str],
) {
    style_detected(cx, parameter, possibilities);
}

/// `style_detected(detected)`: `detected` is one style or several possible
/// ones.
pub fn style_detected(cx: &mut Investigation<'_, '_>, parameter: &str, detected: &[&str]) {
    if no_acceptable_style(cx) {
        return;
    }
    let updated: Vec<String> = match &cx.auto_gen.detected_style {
        Some(current) if current.len() == 1 && detected.len() == 1 && current[0] == detected[0] => {
            current.clone()
        }
        Some(current) => current
            .iter()
            .filter(|style| detected.contains(&style.as_str()))
            .cloned()
            .collect(),
        None => detected.iter().map(|s| (*s).to_owned()).collect(),
    };
    match updated.first() {
        None => no_acceptable_style_bang(cx),
        Some(first) => {
            let first = first.clone();
            cx.auto_gen.detected_style = Some(updated);
            cx.config_to_allow_offenses()
                .insert(parameter.into(), first.into());
        }
    }
}

/// `no_acceptable_style?`.
pub fn no_acceptable_style(cx: &Investigation<'_, '_>) -> bool {
    cx.auto_gen.config_to_allow_offenses.get("Enabled") == Some(&serde_yaml_ng::Value::Bool(false))
}

/// `no_acceptable_style!` (also `conflicting_styles_detected` and
/// `unrecognized_style_detected`).
pub fn no_acceptable_style_bang(cx: &mut Investigation<'_, '_>) {
    let mut config = serde_yaml_ng::Mapping::new();
    config.insert("Enabled".into(), false.into());
    *cx.config_to_allow_offenses() = config;
}

/// `conflicting_styles_detected`.
pub fn conflicting_styles_detected(cx: &mut Investigation<'_, '_>) {
    no_acceptable_style_bang(cx);
}

/// `unrecognized_style_detected`.
pub fn unrecognized_style_detected(cx: &mut Investigation<'_, '_>) {
    no_acceptable_style_bang(cx);
}
