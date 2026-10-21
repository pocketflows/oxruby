//! Port of `lib/rubocop/cop/mixin/method_preference.rb`.

use serde_yaml_ng::{Mapping, Value};

use crate::config_loader::default_configuration;
use crate::cop::Investigation;

/// `preferred_methods`: the `PreferredMethods` mapping, without the keys
/// that are a value the user added to the defaults.
pub fn preferred_methods(cx: &Investigation<'_, '_>) -> Vec<(String, String)> {
    let empty = Mapping::new();
    let default = default_configuration()
        .get(cx.cop_name())
        .and_then(|c| c.get("PreferredMethods"))
        .and_then(Value::as_mapping)
        .unwrap_or(&empty);
    let merged = cx
        .cop_config_value("PreferredMethods")
        .and_then(Value::as_mapping)
        .unwrap_or(&empty);
    let default_values: Vec<&Value> = default.values().collect();
    let overrides: Vec<&Value> = merged
        .values()
        .filter(|v| !default_values.contains(v))
        .collect();
    merged
        .iter()
        .filter(|(key, _)| !overrides.contains(key))
        .filter_map(|(key, value)| Some((scalar(key)?, scalar(value)?)))
        .collect()
}

/// `preferred_method(method)`.
pub fn preferred_method(cx: &Investigation<'_, '_>, method: &str) -> Option<String> {
    preferred_methods(cx)
        .into_iter()
        .find(|(key, _)| key == method)
        .map(|(_, value)| value)
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
