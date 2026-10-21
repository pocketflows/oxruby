//! Port of `lib/rubocop/cop/mixin/interpolation.rb`. A cop lists
//! [`NODE_TYPES`] in `node_types` and, from `on_node`, calls its
//! `on_interpolation` for each node [`interpolations`] returns.

use crate::ast::{Node, NodeType};

/// `on_dstr`, `on_xstr`, `on_dsym` and `on_regexp`.
pub const NODE_TYPES: &[NodeType] = &[
    NodeType::Dstr,
    NodeType::Xstr,
    NodeType::Dsym,
    NodeType::Regexp,
];

/// `on_node_with_interpolations(node)`: the `begin` children of `node`.
pub fn interpolations(node: Node<'_>) -> Vec<Node<'_>> {
    node.child_nodes()
        .filter(|c| c.is(NodeType::Begin))
        .collect()
}
