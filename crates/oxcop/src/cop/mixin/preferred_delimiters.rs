//! Port of `lib/rubocop/cop/mixin/preferred_delimiters.rb`.

use serde_yaml_ng::Value;

use crate::config::Config;

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
