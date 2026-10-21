//! Port of `lib/rubocop/cop/mixin/string_literals_help.rb`.

use serde_yaml_ng::{Mapping, Value};

use crate::cop::Investigation;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::double_quotes_required;
use crate::regex;

/// `wrong_quotes?(src)`, with the cop's own `EnforcedStyle`.
pub fn wrong_quotes(cx: &Investigation<'_, '_>, src: &str) -> bool {
    if src.starts_with('%') || src.starts_with('?') {
        return false;
    }
    if style(cx) == "single_quotes" {
        !double_quotes_required(src)
    } else {
        // `/" | \\[^'\\] | \#[@{$]/x`
        regex!(r#""|\\[^'\\]|\#[@{$]"#).find(src).is_none()
    }
}

/// `preferred_string_literal`.
pub fn preferred_string_literal(cx: &Investigation<'_, '_>) -> &'static str {
    if enforce_double_quotes(cx) {
        "\"\""
    } else {
        "''"
    }
}

/// `enforce_double_quotes?`
pub fn enforce_double_quotes(cx: &Investigation<'_, '_>) -> bool {
    string_literals_config(cx)
        .and_then(|c| c.get("EnforcedStyle"))
        .and_then(Value::as_str)
        == Some("double_quotes")
}

/// `config.for_enabled_cop('Style/StringLiterals')` (`None` when disabled).
fn string_literals_config<'a>(cx: &Investigation<'a, '_>) -> Option<&'a Mapping> {
    const COP: &str = "Style/StringLiterals";
    cx.config.cop_enabled(COP).then(|| cx.config.for_cop(COP))
}
