//! Port of `lib/rubocop/cop/mixin/array_syntax.rb`.

use crate::ast::{Node, NodeType};

/// `bracketed_array_of?(element_type, node)`.
pub fn bracketed_array_of(element_type: NodeType, node: Node<'_>) -> bool {
    if !node.is_square_brackets() || node.child_nodes().next().is_none() {
        return false;
    }
    node.child_nodes().all(|value| value.is(element_type))
}
