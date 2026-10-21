//! Port of `lib/rubocop/cop/mixin/symbol_help.rb`.

use crate::ast::{Node, NodeType};

/// `hash_key?(node)`
pub fn hash_key(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        parent.is(NodeType::Pair)
            && parent
                .child_nodes()
                .next()
                .is_some_and(|first| first.same_as(node))
    })
}
