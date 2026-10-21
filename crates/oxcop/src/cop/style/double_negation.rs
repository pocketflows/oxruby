//! Port of `lib/rubocop/cop/style/double_negation.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid the use of double negation (`!!`).";

static DOUBLE_NEGATIVE: NodePattern = NodePattern::new("(send (send _ :!) :!)");

#[derive(Default)]
pub struct DoubleNegation;

impl Cop for DoubleNegation {
    fn name(&self) -> &'static str {
        "Style/DoubleNegation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !DOUBLE_NEGATIVE.is_match(node) || !node.is_prefix_bang() {
            return;
        }
        if style(cx) == "allowed_in_returns" && allowed_in_returns(node) {
            return;
        }
        let location = node.loc().selector.unwrap();
        cx.add_offense_with_correction(location, MSG, |corrector| {
            corrector.remove(location);
            corrector.insert_after(node.source_range(), ".nil?");
        });
    }
}

fn allowed_in_returns(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is(NodeType::Return)) || end_of_method_definition(node)
}

fn end_of_method_definition(node: Node<'_>) -> bool {
    let Some(def_node) = find_def_node_from_ascendant(node) else {
        return false;
    };
    let conditional_node = find_conditional_node_from_ascendant(node);
    let target = if def_node.is_send() {
        Some(def_node)
    } else {
        def_node.body()
    };
    let Some(last_child) = target.and_then(find_last_child) else {
        return false;
    };
    if let Some(conditional_node) = conditional_node {
        double_negative_condition_return_value(node, last_child, conditional_node)
    } else if matches!(last_child.ty(), NodeType::Pair | NodeType::Hash)
        || last_child.parent().is_some_and(|p| p.is(NodeType::Array))
    {
        false
    } else {
        last_child.first_line() <= node.first_line()
    }
}

fn find_def_node_from_ascendant(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if parent.is_any_def() {
        return Some(parent);
    }
    if define_method(parent) {
        return parent.child_nodes().next();
    }
    find_def_node_from_ascendant(parent)
}

fn define_method(node: Node<'_>) -> bool {
    if !node.is_any_block() {
        return false;
    }
    let Some(child) = node.child_nodes().next().filter(|c| c.is_send()) else {
        return false;
    };
    child.is_method("define_method") || child.is_method("define_singleton_method")
}

fn find_conditional_node_from_ascendant(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors().find(|a| a.is_conditional())
}

fn find_last_child(node: Node<'_>) -> Option<Node<'_>> {
    match node.ty() {
        NodeType::Rescue => node.body().and_then(find_last_child),
        NodeType::Ensure => node.child_nodes().next().and_then(find_last_child),
        _ => node.child_nodes().last(),
    }
}

fn double_negative_condition_return_value(
    node: Node<'_>,
    last_child: Node<'_>,
    conditional_node: Node<'_>,
) -> bool {
    match find_parent_not_enumerable(node) {
        Some(parent) if parent.is(NodeType::Begin) => node.first_line() == parent.last_line(),
        _ => last_child.last_line() <= conditional_node.last_line(),
    }
}

fn find_parent_not_enumerable(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if matches!(parent.ty(), NodeType::Pair | NodeType::Hash | NodeType::Array) {
        find_parent_not_enumerable(parent)
    } else {
        Some(parent)
    }
}
