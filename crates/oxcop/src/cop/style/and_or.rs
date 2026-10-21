//! Port of `lib/rubocop/cop/style/and_or.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

#[derive(Default)]
pub struct AndOr;

impl Cop for AndOr {
    fn name(&self) -> &'static str {
        "Style/AndOr"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::And,
            NodeType::Or,
            NodeType::If,
            NodeType::While,
            NodeType::WhilePost,
            NodeType::Until,
            NodeType::UntilPost,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_operator_keyword() {
            if style(cx) == "always" {
                process_logical_operator(cx, node);
            }
        } else if style(cx) == "conditionals" {
            let Some(condition) = node.condition() else {
                return;
            };
            for operator in condition.each_node().filter(|n| n.is_operator_keyword()) {
                process_logical_operator(cx, operator);
            }
        }
    }
}

fn alternate_operator(node: Node<'_>) -> &'static str {
    match (node.ty(), node.is_logical_operator()) {
        (NodeType::And, true) => "and",
        (NodeType::And, false) => "&&",
        (_, true) => "or",
        (_, false) => "||",
    }
}

fn process_logical_operator(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if node.is_logical_operator() {
        return;
    }
    let message = format!(
        "Use `{}` instead of `{}`.",
        alternate_operator(node),
        node.operator().unwrap_or("")
    );
    let operator = node.loc().operator.unwrap();
    cx.add_offense_with_correction(operator, &message, |corrector| {
        for expr in node.child_nodes() {
            if expr.is_send() {
                correct_send(expr, corrector);
            } else if matches!(
                expr.ty(),
                NodeType::Return | NodeType::Next | NodeType::Break | NodeType::Yield
            ) || expr.is_assignment()
            {
                correct_other(expr, corrector);
            }
        }
        corrector.replace(operator, alternate_operator(node));
        keep_operator_precedence(corrector, node);
    });
}

fn correct_send(node: Node<'_>, corrector: &mut Corrector) {
    if node.is_method("!") {
        correct_not(node, node.receiver(), corrector);
        return;
    }
    if node.is_setter_method() {
        corrector.insert_before(node.receiver().unwrap().source_range(), "(");
        corrector.insert_after(node.last_argument().unwrap().source_range(), ")");
        return;
    }
    if node.is_comparison_method() {
        correct_other(node, corrector);
        return;
    }
    if !correctable_send(node) {
        return;
    }
    let whitespace_before_arg_range = whitespace_before_arg(node);
    corrector.remove(whitespace_before_arg_range);
    corrector.insert_before(whitespace_before_arg_range, "(");
    corrector.insert_after(node.last_argument().unwrap().source_range(), ")");
}

fn correct_not(node: Node<'_>, receiver: Option<Node<'_>>, corrector: &mut Corrector) {
    if node.is_prefix_bang() {
        if let Some(receiver) = receiver.filter(|r| r.is_send()) {
            correct_send(receiver, corrector);
        }
    } else if node.is_prefix_not() {
        correct_other(node, corrector);
    }
}

fn correct_other(node: Node<'_>, corrector: &mut Corrector) {
    if node.is_parenthesized_call() {
        return;
    }
    if node.children().is_empty()
        && node
            .parent()
            .is_some_and(|p| p.child_from_end(1) == Some(node))
    {
        return;
    }
    corrector.wrap(node.source_range(), "(", ")");
}

fn keep_operator_precedence(corrector: &mut Corrector, node: Node<'_>) {
    if node.is(NodeType::Or) && node.parent().is_some_and(|p| p.is(NodeType::And)) {
        corrector.wrap(node.source_range(), "(", ")");
    } else if node.is(NodeType::And)
        && let Some(rhs) = node.rhs().filter(|r| r.is(NodeType::Or))
    {
        corrector.wrap(rhs.source_range(), "(", ")");
    }
}

fn correctable_send(node: Node<'_>) -> bool {
    !node.is_parenthesized() && node.has_arguments() && !node.is_method("[]")
}

fn whitespace_before_arg(node: Node<'_>) -> Range {
    let begin_paren = node.loc().selector.unwrap().end_pos;
    let mut end_paren = begin_paren;
    if regex!(r"\?\S").find(node.source()).is_none() {
        end_paren += 1;
    }
    range_between(begin_paren, end_paren)
}
