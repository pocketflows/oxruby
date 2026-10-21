//! Port of `lib/rubocop/cop/style/require_order.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_with_comments_and_lines;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct RequireOrder;

impl Cop for RequireOrder {
    fn name(&self) -> &'static str {
        "Style/RequireOrder"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["require", "require_relative"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(parent) = node.parent() else {
            return;
        };
        if !node.has_arguments() || not_modifier_form(parent) {
            return;
        }
        let Some(previous_older_sibling) = find_previous_older_sibling(node) else {
            return;
        };
        let message = format!(
            "Sort `{}` in alphabetical order.",
            node.method_name().unwrap_or("")
        );
        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let range1 = range_with_comments_and_lines(ps, previous_older_sibling);
            let range2 = range_with_comments_and_lines(ps, search_node(node));
            corrector.remove(range2);
            corrector.insert_before(range1, ps.slice(range2));
        });
    }
}

fn not_modifier_form(node: Node<'_>) -> bool {
    node.is(NodeType::If) && !node.is_modifier_form()
}

fn search_node(node: Node<'_>) -> Node<'_> {
    let parent = node.parent().unwrap();
    if parent.is(NodeType::If) { parent } else { node }
}

fn require_send(node: Node<'_>) -> bool {
    node.is_send()
        && node.receiver().is_none()
        && matches!(node.method_name(), Some("require" | "require_relative"))
        && node.children().len() == 3
}

/// `if_inside_only_require`.
fn if_inside_only_require(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(e) = node.child(2).filter(|e| require_send(*e)) {
        return Some(e);
    }
    node.child(1).filter(|i| require_send(*i))
}

fn sibling_node(node: Node<'_>) -> Option<Node<'_>> {
    if not_modifier_form(node) {
        return None;
    }
    if node.is(NodeType::If) {
        if_inside_only_require(node)
    } else {
        Some(node)
    }
}

fn find_previous_older_sibling(node: Node<'_>) -> Option<Node<'_>> {
    let method = node.method_name()?;
    for original in search_node(node).left_siblings().into_iter().rev() {
        let sibling = sibling_node(original)?;
        if !(sibling.is_send() && sibling.is_method(method)) {
            return None;
        }
        if !(sibling.has_arguments() && sibling.receiver().is_none()) {
            return None;
        }
        if !in_same_section(sibling, node) {
            return None;
        }
        let (Some(a), Some(b)) = (node.first_argument(), sibling.first_argument()) else {
            return None;
        };
        if !(a.is(NodeType::Str) && b.is(NodeType::Str)) {
            return None;
        }
        let a = a.value().and_then(|v| v.str_bytes()).unwrap_or_default();
        let b = b.value().and_then(|v| v.str_bytes()).unwrap_or_default();
        if a < b {
            return Some(original);
        }
    }
    None
}

fn in_same_section(node1: Node<'_>, node2: Node<'_>) -> bool {
    !node1
        .range_source(node1.source_range().join(node2.source_range().end()))
        .contains("\n\n")
}
