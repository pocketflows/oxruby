//! Port of `lib/rubocop/cop/style/negated_if_else_condition.rb`.

use std::collections::HashSet;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::range_help::{range_between, range_by_whole_lines};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const NEGATED_EQUALITY_METHODS: &[&str] = &["!=", "!~"];

static DOUBLE_NEGATION: NodePattern = NodePattern::new("(send (send _ :!) :!)");

#[derive(Default)]
pub struct NegatedIfElseCondition {
    corrected_nodes: HashSet<NodeId>,
}

impl Cop for NegatedIfElseCondition {
    fn name(&self) -> &'static str {
        "Style/NegatedIfElseCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/InverseMethods", "Style/Not"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.corrected_nodes.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !if_else(node) {
            return;
        }
        let Some(condition) = unwrap_begin_nodes(node.condition()) else {
            return;
        };
        if DOUBLE_NEGATION.is_match(condition) || !negated_condition(condition) {
            return;
        }
        if condition.arguments().len() >= 2 {
            return;
        }
        let kind = if node.is_ternary() { "ternary" } else { "if-else" };
        let message = format!("Invert the negated condition and swap the {kind} branches.");
        let ps = cx.processed_source;
        let corrected_nodes = &mut self.corrected_nodes;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if node
                .ancestors()
                .any(|a| a.is(NodeType::If) && corrected_nodes.contains(&a.id()))
            {
                return;
            }
            correct_negated_condition(corrector, condition);
            swap_branches(ps, corrector, node);
            corrected_nodes.insert(node.id());
        });
    }
}

fn if_else(node: Node<'_>) -> bool {
    let Some(else_branch) = node.else_branch() else {
        return false;
    };
    !node.is_elsif() && (!else_branch.is(NodeType::If) || !else_branch.is_elsif())
}

fn unwrap_begin_nodes(mut node: Option<Node<'_>>) -> Option<Node<'_>> {
    while let Some(n) = node {
        if !matches!(n.ty(), NodeType::Begin | NodeType::Kwbegin) {
            break;
        }
        node = n.child(0);
    }
    node
}

fn negated_condition(node: Node<'_>) -> bool {
    node.is_send()
        && (node.is_negation_method()
            || node
                .method_name()
                .is_some_and(|m| NEGATED_EQUALITY_METHODS.contains(&m)))
}

fn correct_negated_condition(corrector: &mut Corrector, node: Node<'_>) {
    let replacement = if node.is_negation_method() {
        node.receiver().unwrap().source().to_owned()
    } else {
        let inverted_method = node.method_name().unwrap().replacen('!', "=", 1);
        format!(
            "{} {inverted_method} {}",
            node.receiver().unwrap().source(),
            node.first_argument().unwrap().source()
        )
    };
    corrector.replace(node.source_range(), replacement);
}

fn swap_branches(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.if_branch().is_none() {
        corrector.remove(range_by_whole_lines(ps, node.loc().else_.unwrap(), true));
    } else {
        corrector.swap(ps.buffer_source(), if_range(node), else_range(node));
    }
}

fn if_range(node: Node<'_>) -> Range {
    if node.is_ternary() {
        node.if_branch().unwrap().source_range()
    } else {
        range_between(
            node.condition().unwrap().source_range().end_pos,
            node.loc().else_.unwrap().begin_pos,
        )
    }
}

fn else_range(node: Node<'_>) -> Range {
    if node.is_ternary() {
        node.else_branch().unwrap().source_range()
    } else {
        range_between(
            node.loc().else_.unwrap().end_pos,
            node.loc().end.unwrap().begin_pos,
        )
    }
}
