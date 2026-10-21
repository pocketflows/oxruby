//! Port of `lib/rubocop/cop/mixin/preferred_delimiters.rb`.

use serde_yaml_ng::{Mapping, Value};

use crate::config::Config;
use crate::cop::base::raise;

pub const PERCENT_LITERAL_TYPES: &[&str] =
    &["%", "%i", "%I", "%q", "%Q", "%r", "%s", "%w", "%W", "%x"];

/// `PreferredDelimiters.new(type, config, nil).delimiters`: the opening and
/// closing delimiter for a percent literal type (`%w`, ...), from
/// `Style/PercentLiteralDelimiters`' `PreferredDelimiters`.
pub fn delimiters(config: &Config, ty: &str) -> Option<(String, String)> {
    let preferred = config
        .for_cop("Style/PercentLiteralDelimiters")
        .get("PreferredDelimiters")?
        .as_mapping()?;
    ensure_valid_preferred_delimiters(preferred);
    let lookup = |key: &str| preferred.get(key).and_then(Value::as_str);
    let value = if preferred.contains_key("default") {
        lookup(ty).or_else(|| lookup("default"))
    } else {
        lookup(ty)
    }?;
    let mut chars = value.chars();
    let open = chars.next()?.to_string();
    let close = chars.next().map_or_else(String::new, |c| c.to_string());
    Some((open, close))
}

/// `ensure_valid_preferred_delimiters`: raises `ArgumentError` for keys
/// other than the percent literal types and `default`.
fn ensure_valid_preferred_delimiters(preferred: &Mapping) {
    let invalid: Vec<&str> = preferred
        .keys()
        .filter_map(Value::as_str)
        .filter(|key| *key != "default" && !PERCENT_LITERAL_TYPES.contains(key))
        .collect();
    if !invalid.is_empty() {
        raise(
            "ArgumentError",
            format!(
                "Invalid preferred delimiter config key: {}",
                invalid.join(", ")
            ),
        );
    }
}
