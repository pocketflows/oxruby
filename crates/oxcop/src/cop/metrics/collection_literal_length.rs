//! Port of `lib/rubocop/cop/metrics/collection_literal_length.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid hard coding large quantities of data in code. \
                   Prefer reading the data from an external source.";

#[derive(Default)]
pub struct CollectionLiteralLength;

impl Cop for CollectionLiteralLength {
    fn name(&self) -> &'static str {
        "Metrics/CollectionLiteralLength"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array, NodeType::Hash, NodeType::Index, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let threshold = collection_threshold(cx);
        match node.ty() {
            NodeType::Array | NodeType::Hash => {
                if node.children().len() as f64 >= threshold {
                    cx.add_offense(node.source_range(), MSG);
                }
            }
            _ => {
                if node.is(NodeType::Send) && !node.is_method("[]") {
                    return;
                }
                if !node.receiver().is_some_and(set_const) {
                    return;
                }
                if node.arguments().len() as f64 >= threshold {
                    cx.add_offense(node.source_range(), MSG);
                }
            }
        }
    }
}

/// `(const {cbase nil?} :Set)`
fn set_const(node: Node<'_>) -> bool {
    node.is_global_const("Set")
}

/// `cop_config.fetch('Max', Float::INFINITY)`.
fn collection_threshold(cx: &Investigation<'_, '_>) -> f64 {
    match cx.cop_config_value("Max") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::INFINITY),
        _ => f64::INFINITY,
    }
}
