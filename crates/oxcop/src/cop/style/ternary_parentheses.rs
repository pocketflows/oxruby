//! Port of `lib/rubocop/cop/style/ternary_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::safe_assignment::{safe_assignment, safe_assignment_allowed};
use crate::cop::{Cop, Corrector, Investigation};
use crate::regex;

const NON_COMPLEX_TYPES: &[NodeType] = &[
    NodeType::Ivar,
    NodeType::Gvar,
    NodeType::Cvar,
    NodeType::Lvar,
    NodeType::Const,
    NodeType::Defined,
    NodeType::Yield,
];

#[derive(Default)]
pub struct TernaryParentheses;

impl Cop for TernaryParentheses {
    fn name(&self) -> &'static str {
        "Style/TernaryParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(condition) = node.condition() else {
            return;
        };
        if only_closing_parenthesis_is_last_line(condition)
            || condition_as_parenthesized_one_line_pattern_matching(cx, condition)
        {
            return;
        }
        if !(node.is_ternary() && offense(cx, node)) {
            return;
        }
        let style = style(cx);
        let message = if style == "require_parentheses_when_complex" {
            let command = if parenthesized(condition) {
                "Only use"
            } else {
                "Use"
            };
            format!("{command} parentheses for ternary expressions with complex conditions.")
        } else {
            let command = if style == "require_parentheses" {
                "Use"
            } else {
                "Omit"
            };
            format!("{command} parentheses for ternary conditions.")
        };
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            autocorrect(src, corrector, condition);
        });
    }
}

fn only_closing_parenthesis_is_last_line(condition: Node<'_>) -> bool {
    condition
        .source()
        .split('\n')
        .rev()
        .find(|line| !line.is_empty())
        == Some(")")
}

fn condition_as_parenthesized_one_line_pattern_matching(
    cx: &Investigation<'_, '_>,
    condition: Node<'_>,
) -> bool {
    if !condition.is_parenthesized_call() {
        return false;
    }
    let Some(first_child) = condition.child(0) else {
        return false;
    };
    if cx.target_ruby_version() >= 3.0 {
        first_child.is(NodeType::MatchPatternP)
    } else {
        first_child.is(NodeType::MatchPattern)
    }
}

fn autocorrect(src: &str, corrector: &mut Corrector, condition: Node<'_>) {
    if parenthesized(condition) && (safe_assignment(condition) || unsafe_autocorrect(condition)) {
        return;
    }
    if parenthesized(condition) {
        correct_parenthesized(src, corrector, condition);
    } else {
        corrector.wrap(condition.source_range(), "(", ")");
    }
}

fn offense(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let condition = node.condition().unwrap();
    if parenthesized_modifier_condition(condition) {
        return false;
    }
    if safe_assignment(condition) {
        return !safe_assignment_allowed(cx);
    }
    let parens = parenthesized(condition);
    match style(cx) {
        "require_parentheses_when_complex" => {
            if complex_condition(condition) {
                !parens
            } else {
                parens
            }
        }
        "require_parentheses" => !parens,
        _ => parens,
    }
}

fn complex_condition(condition: Node<'_>) -> bool {
    if condition.is(NodeType::Begin) {
        condition.child_nodes().any(complex_condition)
    } else {
        !non_complex_expression(condition)
    }
}

fn non_complex_expression(condition: Node<'_>) -> bool {
    condition.is_any(NON_COMPLEX_TYPES) || non_complex_send(condition)
}

fn non_complex_send(node: Node<'_>) -> bool {
    if !node.is_call() {
        return false;
    }
    !node.is_operator_method() || node.is_method("[]")
}

fn parenthesized(node: Node<'_>) -> bool {
    node.is(NodeType::Begin)
}

fn unsafe_autocorrect(condition: Node<'_>) -> bool {
    condition.child_nodes().any(below_ternary_precedence)
}

fn parenthesized_modifier_condition(condition: Node<'_>) -> bool {
    if !condition.is(NodeType::Begin) {
        return false;
    }
    condition
        .child(0)
        .is_some_and(|inner| inner.is(NodeType::If) && inner.is_modifier_form())
}

fn unparenthesized_method_call(child: Node<'_>) -> bool {
    method_name(child).is_some_and(|name| regex!(r"^[a-zA-Z]").find(name).is_some())
        && !child.is_parenthesized()
}

fn below_ternary_precedence(child: Node<'_>) -> bool {
    (child.is(NodeType::Or) && child.is_semantic_operator())
        || (child.is(NodeType::And) && child.is_semantic_operator())
        || (child.is_send() && child.is_prefix_not())
}

/// `{($:defined? _ ...) (call {_ nil?} $_ _ ...)}`.
fn method_name(node: Node<'_>) -> Option<&str> {
    if node.is(NodeType::Defined) {
        return (!node.children().is_empty()).then_some("defined?");
    }
    if node.is_call() && node.children().len() >= 3 {
        return node.method_name();
    }
    None
}

fn correct_parenthesized(src: &str, corrector: &mut Corrector, condition: Node<'_>) {
    let begin = condition.loc().begin.unwrap();
    let end = condition.loc().end.unwrap();
    corrector.remove(begin);
    corrector.remove(end);
    // `processed_source.last_token_of(node).space_after?`: the last token
    // is the closing parenthesis.
    let whitespace_after = src[end.end_pos..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace);
    if !whitespace_after {
        corrector.insert_after(end, " ");
    }
    if let Some(send_node) = condition.child_nodes().last()
        && node_args_need_parens(send_node)
    {
        parenthesize_condition_arguments(corrector, send_node);
    }
}

fn parenthesize_condition_arguments(corrector: &mut Corrector, send_node: Node<'_>) {
    let range_start = if send_node.is(NodeType::Defined) {
        send_node.loc().keyword.unwrap()
    } else {
        send_node.loc().selector.unwrap()
    };
    let opening_range = range_start
        .end()
        .join(send_node.first_argument().unwrap().source_range().begin());
    corrector.replace(opening_range, "(");
    corrector.insert_after(send_node.last_argument().unwrap().source_range(), ")");
}

fn node_args_need_parens(send_node: Node<'_>) -> bool {
    if !(send_node.is_call() || send_node.is(NodeType::Defined)) {
        return false;
    }
    if send_node.arguments().is_empty() || send_node.is_parenthesized() {
        return false;
    }
    send_node.is_dot() || send_node.is_safe_navigation() || unparenthesized_method_call(send_node)
}
