//! Port of `lib/rubocop/cop/lint/require_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use parentheses in the method call to avoid confusion about precedence.";

#[derive(Default)]
pub struct RequireParentheses;

impl Cop for RequireParentheses {
    fn name(&self) -> &'static str {
        "Lint/RequireParentheses"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.has_arguments() || node.is_parenthesized() {
            return;
        }

        let Some(first_argument) = node.first_argument() else {
            return;
        };
        if first_argument.is(NodeType::If) && first_argument.is_ternary() {
            check_ternary(cx, first_argument, node);
        } else if node.is_predicate_method()
            && let Some(last_argument) = node.last_argument()
        {
            check_predicate(cx, last_argument, node);
        }
    }
}

fn check_ternary(cx: &mut Investigation<'_, '_>, ternary: Node<'_>, node: Node<'_>) {
    let Some(condition) = ternary.condition() else {
        return;
    };
    if node.is_method("[]") || node.is_assignment_method() || !condition.is_operator_keyword() {
        return;
    }

    let range = range_between(
        node.source_range().begin_pos,
        condition.source_range().end_pos,
    );
    cx.add_offense(range, MSG);
}

fn check_predicate(cx: &mut Investigation<'_, '_>, predicate: Node<'_>, node: Node<'_>) {
    if !predicate.is_operator_keyword() {
        return;
    }

    cx.add_offense(node.source_range(), MSG);
}
