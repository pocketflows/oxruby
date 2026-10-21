//! Port of `lib/rubocop/cop/lint/literal_assignment_in_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct LiteralAssignmentInCondition;

impl Cop for LiteralAssignmentInCondition {
    fn name(&self) -> &'static str {
        "Lint/LiteralAssignmentInCondition"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(condition) = node.condition() else {
            return;
        };
        let mut found = Vec::new();
        traverse_node(condition, &mut found);
        for asgn_node in found {
            if asgn_node.loc().operator.is_none() {
                continue;
            }
            let Some(rhs) = asgn_node.rhs() else {
                continue;
            };
            if !all_literals(rhs) || parallel_assignment_with_splat_operator(rhs) {
                continue;
            }
            let range = offense_range(asgn_node, rhs);
            let message = format!(
                "Don't use literal assignment `= {}` in conditional, should be `==` or non-literal operand.",
                rhs.source()
            );
            cx.add_offense(range, &message);
        }
    }
}

fn traverse_node<'a>(node: Node<'a>, found: &mut Vec<Node<'a>>) {
    if node.is_equals_asgn() {
        found.push(node);
    }
    for child in node.child_nodes() {
        if scope_body(node, child) {
            continue;
        }
        traverse_node(child, found);
    }
}

fn scope_body(node: Node<'_>, child: Node<'_>) -> bool {
    (node.is_any_block() || node.is_any_def()) && node.body().is_some_and(|b| child.same_as(b))
}

fn all_literals(node: Node<'_>) -> bool {
    match node.ty() {
        NodeType::Dstr | NodeType::Xstr => false,
        NodeType::Array => node.child_nodes().all(all_literals),
        NodeType::Hash => node
            .hash_values()
            .into_iter()
            .chain(node.hash_keys())
            .all(all_literals),
        _ => node.is_literal(),
    }
}

fn parallel_assignment_with_splat_operator(node: Node<'_>) -> bool {
    node.is(NodeType::Array) && node.child(0).is_some_and(|v| v.is(NodeType::Splat))
}

fn offense_range(asgn_node: Node<'_>, rhs: Node<'_>) -> Range {
    asgn_node
        .loc()
        .operator
        .unwrap()
        .join(rhs.source_range().end())
}
