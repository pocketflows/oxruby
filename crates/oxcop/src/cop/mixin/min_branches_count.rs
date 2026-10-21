//! Port of `lib/rubocop/cop/mixin/min_branches_count.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;

/// `min_branches_count?(node)` for `case` and `if` nodes.
pub fn min_branches_count(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let branches = if node.is(NodeType::Case) {
        node.when_branches().len()
    } else if node.is(NodeType::If) {
        let mut branches = Vec::new();
        if_conditional_branches(Some(node), &mut branches);
        branches.len()
    } else {
        panic!("Unsupported {:?} node type", node.ty().name());
    };
    branches >= configured_min_branches_count(cx)
}

fn configured_min_branches_count(cx: &Investigation<'_, '_>) -> usize {
    match cx.cop_config_value("MinBranchesCount") {
        None | Some(serde_yaml_ng::Value::Null) => 3,
        Some(value) => match value.as_i64() {
            Some(length) if length > 0 => length as usize,
            _ => panic!("MinBranchesCount needs to be a positive integer!"),
        },
    }
}

/// `if_conditional_branches(node)`: the `if` branches of an `if`/`elsif`
/// chain.
pub fn if_conditional_branches<'a>(node: Option<Node<'a>>, branches: &mut Vec<Option<Node<'a>>>) {
    let Some(node) = node.filter(|n| n.is(NodeType::If)) else {
        return;
    };
    branches.push(node.if_branch());
    if let Some(else_branch) = node.else_branch().filter(|e| e.is(NodeType::If)) {
        if_conditional_branches(Some(else_branch), branches);
    }
}
