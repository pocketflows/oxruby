//! Port of `lib/rubocop/cop/style/nested_modifier.rb`.

use crate::ast::node::COMPARISON_OPERATORS;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Avoid using nested modifiers.";

#[derive(Default)]
pub struct NestedModifier {
    ignored: IgnoredNodes,
}

impl Cop for NestedModifier {
    fn name(&self) -> &'static str {
        "Style/NestedModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::While, NodeType::Until, NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.part_of(node) {
            return;
        }
        if !(modifier(Some(node)) && modifier(node.parent())) {
            return;
        }
        cx.add_offense_with_correction(node.loc().keyword.unwrap(), MSG, |corrector| {
            let parent = node.parent().unwrap();
            if !(node.is(NodeType::If) && parent.is(NodeType::If)) {
                return;
            }
            let range = Range::new(
                node.loc().keyword.unwrap().begin_pos,
                parent.condition().unwrap().source_range().end_pos,
            );
            corrector.replace(range, new_expression(node));
        });
        self.ignored.ignore(node);
    }
}

fn modifier(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| n.is_basic_conditional() && n.is_modifier_form())
}

fn new_expression(inner: Node<'_>) -> String {
    let outer = inner.parent().unwrap();
    let operator = if outer.keyword() == "if" { "&&" } else { "||" };
    let lh_operand = left_hand_operand(outer, operator);
    let rh_operand = right_hand_operand(inner, outer.keyword());
    format!("{} {lh_operand} {operator} {rh_operand}", outer.keyword())
}

fn left_hand_operand(node: Node<'_>, operator: &str) -> String {
    let condition = node.condition().unwrap();
    let expr = condition.source().to_owned();
    if condition.is(NodeType::Or) && operator == "&&" {
        format!("({expr})")
    } else {
        expr
    }
}

fn right_hand_operand(node: Node<'_>, left_hand_keyword: &str) -> String {
    let condition = node.condition().unwrap();
    let negated = left_hand_keyword != node.keyword();
    let mut expr = if condition.is_send()
        && !condition.arguments().is_empty()
        && !condition.is_operator_method()
    {
        add_parentheses_to_method_arguments(condition)
    } else {
        condition.source().to_owned()
    };
    if requires_parens(condition, negated) {
        expr = format!("({expr})");
    }
    if negated {
        expr = format!("!{expr}");
    }
    expr
}

fn add_parentheses_to_method_arguments(send: Node<'_>) -> String {
    let mut expr = String::new();
    if let Some(receiver) = send.receiver() {
        expr.push_str(&format!("{}.", receiver.source()));
    }
    expr.push_str(send.method_name().unwrap_or(""));
    let args: Vec<&str> = send.arguments().iter().map(|a| a.source()).collect();
    expr.push_str(&format!("({})", args.join(", ")));
    expr
}

fn requires_parens(node: Node<'_>, negated: bool) -> bool {
    (negated && node.is_operator_keyword())
        || node.is(NodeType::Or)
        || node
            .children()
            .iter()
            .any(|c| matches!(c, Child::Sym(s) if COMPARISON_OPERATORS.contains(&s.as_str())))
}
