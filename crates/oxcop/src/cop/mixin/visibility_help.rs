//! Port of `lib/rubocop/cop/mixin/visibility_help.rb`.

use crate::ast::{Node, NodeType};

const VISIBILITY_SCOPES: &[&str] = &["private", "protected", "public"];

fn is_visibility_send(node: Node<'_>) -> bool {
    node.is_send()
        && node.receiver().is_none()
        && node
            .method_name()
            .is_some_and(|m| VISIBILITY_SCOPES.contains(&m))
}

/// `visibility_block?`: `(send nil? VISIBILITY_SCOPES)`.
pub fn visibility_block(node: Node<'_>) -> bool {
    is_visibility_send(node) && node.children().len() == 2
}

/// `visibility_inline_on_def?`: `(send nil? VISIBILITY_SCOPES def)`.
pub fn visibility_inline_on_def(node: Node<'_>) -> bool {
    is_visibility_send(node)
        && node.children().len() == 3
        && node.child(2).is_some_and(|c| c.is(NodeType::Def))
}

/// `visibility_inline_on_method_name?(node, method_name:)`.
pub fn visibility_inline_on_method_name(node: Node<'_>, method_name: &str) -> bool {
    is_visibility_send(node)
        && node.children().len() == 3
        && node
            .child(2)
            .is_some_and(|c| c.is(NodeType::Sym) && c.sym_value() == Some(method_name))
}

/// `node_visibility(node)`: `private`, `protected` or `public`.
pub fn node_visibility<'a>(node: Node<'a>) -> &'a str {
    node_visibility_from_visibility_inline(node)
        .or_else(|| find_visibility_start(node).and_then(|n| n.method_name()))
        .unwrap_or("public")
}

fn node_visibility_from_visibility_inline<'a>(node: Node<'a>) -> Option<&'a str> {
    if !node.is(NodeType::Def) {
        return None;
    }
    if let Some(parent) = node.parent()
        && visibility_inline_on_def(parent)
    {
        return parent.method_name();
    }
    let name = node.method_name()?;
    node.right_siblings()
        .into_iter()
        .rev()
        .find(|s| visibility_inline_on_method_name(*s, name))
        .and_then(|s| s.method_name())
}

/// `find_visibility_start(node)`.
pub fn find_visibility_start(node: Node<'_>) -> Option<Node<'_>> {
    node.left_siblings()
        .into_iter()
        .rev()
        .find(|s| visibility_block(*s))
}

/// `find_visibility_end(node)`.
pub fn find_visibility_end(node: Node<'_>) -> Option<Node<'_>> {
    let visibility = node_visibility(node);
    let right = node.right_siblings();
    right
        .iter()
        .copied()
        .find(|child| {
            let v = node_visibility(*child);
            v != visibility && VISIBILITY_SCOPES.contains(&v)
        })
        .or_else(|| right.last().copied())
}
