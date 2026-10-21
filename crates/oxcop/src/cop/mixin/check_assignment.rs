//! Port of `lib/rubocop/cop/mixin/check_assignment.rb`.
//!
//! The Ruby mixin defines `on_lvasgn` (and friends) and `on_send`, calling
//! the cop's `check_assignment(node, rhs)`. Here a cop adds
//! [`NODE_TYPES`] to its `node_types` and calls [`assignment`] from
//! `on_node` to get the `(node, rhs)` it would have been called with.

use crate::ast::{Node, NodeType};

/// The node types the mixin's callbacks handle.
pub const NODE_TYPES: &[NodeType] = &[
    NodeType::Lvasgn,
    NodeType::Ivasgn,
    NodeType::Cvasgn,
    NodeType::Gvasgn,
    NodeType::Casgn,
    NodeType::Masgn,
    NodeType::OpAsgn,
    NodeType::OrAsgn,
    NodeType::AndAsgn,
    NodeType::Send,
];

/// The `check_assignment(node, rhs)` call the mixin would make for `node`:
/// `None` when it makes none. For assignments `rhs` may be `nil`; for
/// `send` there is no call without an rhs.
pub fn assignment(node: Node<'_>) -> Option<(Node<'_>, Option<Node<'_>>)> {
    if node.is(NodeType::Send) {
        let rhs = extract_rhs(node)?;
        return Some((node, Some(rhs)));
    }
    NODE_TYPES
        .contains(&node.ty())
        .then(|| (node, extract_rhs(node)))
}

/// `extract_rhs(node)`.
pub fn extract_rhs(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_call() {
        node.last_argument()
    } else if node.is_assignment() {
        node.expression()
    } else {
        None
    }
}
