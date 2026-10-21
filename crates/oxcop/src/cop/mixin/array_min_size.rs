//! Port of `lib/rubocop/cop/mixin/array_min_size.rb`.
//!
//! Ruby keeps `largest_brackets` on the cop class; here it lives in the
//! cop's [`ArrayStyleState`], next to `@smallest_percent`.

use serde_yaml_ng::Value;

use crate::ast::Node;
use crate::cop::Investigation;

/// `self.class.largest_brackets` and `@smallest_percent` (`None` is
/// `-Float::INFINITY` and `Float::INFINITY`).
#[derive(Debug, Default)]
pub struct ArrayStyleState {
    largest_brackets: Option<i64>,
    smallest_percent: Option<i64>,
}

/// `below_array_length?(node)`.
pub fn below_array_length(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    (node.child_nodes().count() as i64) < min_size_config(cx)
}

/// `min_size_config`.
pub fn min_size_config(cx: &Investigation<'_, '_>) -> i64 {
    cx.cop_config_int("MinSize").unwrap_or(0)
}

/// `array_style_detected(style, ary_size)`: `style` is `"percent"` or
/// `"brackets"`.
pub fn array_style_detected(
    cx: &mut Investigation<'_, '_>,
    state: &mut ArrayStyleState,
    style: &str,
    ary_size: i64,
) {
    if cx.config_to_allow_offenses().get("Enabled") == Some(&Value::Bool(false)) {
        return;
    }
    if style == "brackets" && state.largest_brackets.is_none_or(|l| ary_size > l) {
        state.largest_brackets = Some(ary_size);
    }
    if style == "percent" && state.smallest_percent.is_none_or(|s| ary_size < s) {
        state.smallest_percent = Some(ary_size);
    }
    let cfg = cx.config_to_allow_offenses();
    let enforced = cfg
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match enforced {
        Some(enforced) if enforced == style => {}
        None if !cfg.contains_key("EnforcedStyle") => {
            cfg.insert("EnforcedStyle".into(), style.into());
        }
        _ => {
            // `smallest_percent <= largest_brackets`, with the infinities.
            let disable = match (state.smallest_percent, state.largest_brackets) {
                (Some(percent), Some(brackets)) => percent <= brackets,
                _ => false,
            };
            if disable {
                cfg.clear();
                cfg.insert("Enabled".into(), false.into());
            } else {
                cfg.insert("EnforcedStyle".into(), "percent".into());
                if let Some(brackets) = state.largest_brackets {
                    cfg.insert("MinSize".into(), (brackets + 1).into());
                }
            }
        }
    }
}
