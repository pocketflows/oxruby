//! Port of `lib/rubocop/cop/lint/unexpected_block_arity.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct UnexpectedBlockArity;

impl Cop for UnexpectedBlockArity {
    fn name(&self) -> &'static str {
        "Lint/UnexpectedBlockArity"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let method_name = node.method_name().unwrap_or_default();
        let Some(expected) = expected_arity(cx, method_name) else {
            return;
        };
        if node.receiver().is_none() {
            return;
        }
        let actual = arg_count(node);
        if actual >= expected as f64 {
            return;
        }

        let message = format!(
            "`{method_name}` expects at least {expected} positional arguments, got {}.",
            actual as i64
        );
        cx.add_offense(node.source_range(), &message);
    }
}

/// `included_method?` and `expected_arity`.
fn expected_arity(cx: &Investigation<'_, '_>, method: &str) -> Option<i64> {
    let Some(Value::Mapping(methods)) = cx.cop_config_value("Methods") else {
        return None;
    };
    methods.get(method).and_then(Value::as_i64)
}

fn arg_count(node: Node<'_>) -> f64 {
    if node.is(NodeType::Numblock) {
        // the maximum numbered param for the block
        return match node.children().get(1) {
            Some(Child::Int(i)) => *i as f64,
            _ => 0.0,
        };
    }
    if node.is(NodeType::Itblock) {
        // `it` block parameter is always one
        return 1.0;
    }

    let mut count = 0.0;
    for arg in node.arguments() {
        if arg.is(NodeType::Restarg) {
            return f64::INFINITY;
        }
        if matches!(arg.ty(), NodeType::Arg | NodeType::Optarg | NodeType::Mlhs) {
            count += 1.0;
        }
    }
    count
}
