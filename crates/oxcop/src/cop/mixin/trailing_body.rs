//! Port of `lib/rubocop/cop/mixin/trailing_body.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::processed_source::Range;

/// `node.to_a.reverse[0]`: the last child, when it is a node.
pub fn last_child(node: Node<'_>) -> Option<Node<'_>> {
    match node.children().last() {
        Some(Child::Node(id)) => Some(node.ast().node(*id)),
        _ => None,
    }
}

/// `trailing_body?(node)`.
pub fn trailing_body(node: Node<'_>) -> bool {
    let Some(body) = last_child(node) else {
        return false;
    };
    node.is_multiline() && body_on_first_line(node, body)
}

/// `body_on_first_line?(node, body)`.
pub fn body_on_first_line(node: Node<'_>, body: Node<'_>) -> bool {
    node.first_line() == body.first_line()
}

/// `first_part_of(body)`.
pub fn first_part_of(body: Node<'_>) -> Range {
    if body.is(NodeType::Begin) {
        body.child(0)
            .map_or(body.source_range(), |c| c.source_range())
    } else {
        body.source_range()
    }
}
