//! Port of `lib/rubocop/cop/lint/deprecated_constants.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct DeprecatedConstants;

impl Cop for DeprecatedConstants {
    fn name(&self) -> &'static str {
        "Lint/DeprecatedConstants"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let constant = node.source();
        let constant = constant.strip_prefix("::").unwrap_or(constant);
        let Some(Value::Mapping(deprecated_constant)) = deprecated_constants(cx, constant) else {
            return;
        };
        let alternative = deprecated_constant.get("Alternative").and_then(scalar);
        let version = deprecated_constant.get("DeprecatedVersion").and_then(scalar);
        let version_f = version.as_deref().map_or(0.0, ruby_to_f);
        if cx.target_ruby_version() < version_f {
            return;
        }
        let message = message(alternative.as_deref(), node.source(), version.as_deref());
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            // `corrector.replace(node, nil)` is a no-op in `TreeRewriter`.
            if let Some(alternative) = &alternative {
                corrector.replace(node.source_range(), alternative);
            }
        });
    }
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `String#to_f`: the leading float, or 0.0.
fn ruby_to_f(s: &str) -> f64 {
    let s = s.trim_start();
    let end = s
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+')))
        .map(|(i, c)| i + c.len_utf8())
        .last()
        .unwrap_or(0);
    let mut candidate = &s[..end];
    while !candidate.is_empty() {
        if let Ok(f) = candidate.parse() {
            return f;
        }
        candidate = &candidate[..candidate.len() - 1];
    }
    0.0
}

fn message(good: Option<&str>, bad: &str, deprecated_version: Option<&str>) -> String {
    let deprecated_message = deprecated_version
        .map(|v| format!(", deprecated since Ruby {v}"))
        .unwrap_or_default();
    match good {
        Some(good) => format!("Use `{good}` instead of `{bad}`{deprecated_message}."),
        None => format!("Do not use `{bad}`{deprecated_message}."),
    }
}

fn deprecated_constants<'a>(cx: &Investigation<'a, '_>, constant: &str) -> Option<&'a Value> {
    match cx.cop_config_value("DeprecatedConstants") {
        Some(Value::Mapping(map)) => map.get(constant),
        _ => None,
    }
}
