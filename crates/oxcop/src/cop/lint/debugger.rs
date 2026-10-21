//! Port of `lib/rubocop/cop/lint/debugger.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct Debugger;

impl Cop for Debugger {
    fn name(&self) -> &'static str {
        "Lint/Debugger"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(debugger_method(cx, node) || debugger_require(cx, node)) {
            return;
        }
        if assumed_usage_context(node) {
            return;
        }
        cx.add_offense(node.source_range(), &message(node));
    }
}

fn message(node: Node<'_>) -> String {
    format!("Remove debugger entry point `{}`.", node.source())
}

/// `config.is_a?(Array) ? config : config.values.flatten`
fn config_list(cx: &Investigation<'_, '_>, key: &str) -> Vec<String> {
    fn flatten(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::Sequence(items) => items.iter().for_each(|item| flatten(item, out)),
            Value::String(s) => out.push(s.clone()),
            Value::Null => {}
            other => {
                if let Ok(s) = serde_yaml_ng::to_string(other) {
                    out.push(s.trim_end().to_owned());
                }
            }
        }
    }
    let mut out = Vec::new();
    match cx.cop_config_value(key) {
        Some(Value::Mapping(map)) => map.values().for_each(|v| flatten(v, &mut out)),
        Some(value) => flatten(value, &mut out),
        None => {}
    }
    out
}

fn debugger_methods(cx: &Investigation<'_, '_>) -> Vec<String> {
    config_list(cx, "DebuggerMethods")
}

fn debugger_requires(cx: &Investigation<'_, '_>) -> Vec<String> {
    config_list(cx, "DebuggerRequires")
}

fn debugger_method(cx: &Investigation<'_, '_>, send_node: Node<'_>) -> bool {
    let methods = debugger_methods(cx);
    let Some(method_name) = send_node.method_name() else {
        return false;
    };
    if !methods
        .iter()
        .any(|method| method.rsplit('.').next() == Some(method_name))
    {
        return false;
    }
    let chained = chained_method_name(send_node);
    methods.contains(&chained)
}

fn debugger_require(cx: &Investigation<'_, '_>, send_node: Node<'_>) -> bool {
    if !(send_node.is_method("require") && send_node.arguments().len() == 1) {
        return false;
    }
    let argument = send_node.first_argument().unwrap();
    if !argument.is(NodeType::Str) {
        return false;
    }
    let Some(value) = argument.str_content() else {
        return false;
    };
    debugger_requires(cx).iter().any(|r| r == value)
}

fn assumed_usage_context(node: Node<'_>) -> bool {
    if !(node.arguments().is_empty() && node.ancestors().any(|a| a.is_call())) {
        return false;
    }
    if assumed_argument(node) {
        return true;
    }
    !node.ancestors().any(|ancestor| {
        ancestor.is_any_block() || ancestor.is(NodeType::Kwbegin) || ancestor.is_lambda_or_proc()
    })
}

fn chained_method_name(send_node: Node<'_>) -> String {
    let mut chained_method_name = send_node.method_name().unwrap_or_default().to_owned();
    let mut receiver = send_node.receiver();
    while let Some(r) = receiver {
        let name = if r.is_send() {
            r.method_name().unwrap_or_default().to_owned()
        } else {
            r.const_name().unwrap_or_default()
        };
        chained_method_name = format!("{name}.{chained_method_name}");
        receiver = r.receiver();
    }
    chained_method_name
}

fn assumed_argument(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is_call() || parent.is_literal() || parent.is(NodeType::Pair)
}
