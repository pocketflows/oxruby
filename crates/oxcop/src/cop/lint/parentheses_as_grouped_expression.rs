//! Port of `lib/rubocop/cop/lint/parentheses_as_grouped_expression.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct ParenthesesAsGroupedExpression;

impl Cop for ParenthesesAsGroupedExpression {
    fn name(&self) -> &'static str {
        "Lint/ParenthesesAsGroupedExpression"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if valid_context(node) {
            return;
        }
        let space_length = spaces_before_left_parenthesis(node);
        if space_length == 0 {
            return;
        }
        let first_argument = node.first_argument().unwrap();
        let range = space_range(first_argument.source_range(), space_length);
        let message = format!(
            "`{}` interpreted as grouped expression.",
            first_argument.source()
        );
        cx.add_offense_with_correction(range, &message, |corrector| corrector.remove(range));
    }
}

fn valid_context(node: Node<'_>) -> bool {
    let arguments = node.arguments();
    if !(arguments.len() == 1 && arguments[0].is_parenthesized_call()) {
        return true;
    }
    let first_argument = arguments[0];
    if first_argument.is_any_block() {
        return true;
    }
    node.is_operator_method()
        || node.is_setter_method()
        || chained_calls(node)
        || valid_first_argument(first_argument)
}

fn valid_first_argument(first_arg: Node<'_>) -> bool {
    first_arg.is_operator_keyword()
        || first_arg.is(NodeType::Hash)
        || ternary_expression(first_arg)
        || compound_range(first_arg)
        || invalid_bare_argument(first_arg)
}

fn compound_range(first_arg: Node<'_>) -> bool {
    first_arg.is_range() && first_arg.is_parenthesized_call()
}

fn invalid_bare_argument(node: Node<'_>) -> bool {
    let mut node = node;
    while node.is(NodeType::Begin) && node.children().len() == 1 {
        match node.child(0) {
            Some(child) => node = child,
            None => return false,
        }
    }
    keyword_operator(node) || modifier_expression(node)
}

fn keyword_operator(node: Node<'_>) -> bool {
    (node.is_operator_keyword() && node.is_semantic_operator())
        || node.is(NodeType::Rescue)
        || (node.is_send() && node.is_prefix_not())
}

fn modifier_expression(node: Node<'_>) -> bool {
    node.is_any(&[NodeType::If, NodeType::While, NodeType::Until]) && node.is_modifier_form()
}

fn chained_calls(node: Node<'_>) -> bool {
    let first_argument = node.first_argument().unwrap();
    first_argument.is_call()
        && node
            .children()
            .last()
            .and_then(|c| c.as_node())
            .is_some_and(|last| node.ast().node(last).children().len() > 1)
}

fn ternary_expression(node: Node<'_>) -> bool {
    node.is(NodeType::If) && node.is_ternary()
}

fn spaces_before_left_parenthesis(node: Node<'_>) -> usize {
    let first_argument = node.first_argument().unwrap();
    if node.is_parenthesized() || !first_argument.source().starts_with('(') {
        return 0;
    }
    first_argument
        .source_range()
        .begin_pos
        .saturating_sub(node.loc().selector.unwrap().end_pos)
}

fn space_range(expr: Range, space_length: usize) -> Range {
    range_between(expr.begin_pos - space_length, expr.begin_pos)
}
