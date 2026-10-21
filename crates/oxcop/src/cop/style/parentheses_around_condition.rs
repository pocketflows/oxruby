//! Port of `lib/rubocop/cop/style/parentheses_around_condition.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::correctors::parentheses_corrector;
use crate::cop::mixin::parentheses::parens_required;
use crate::cop::mixin::safe_assignment::{safe_assignment, safe_assignment_allowed};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct ParenthesesAroundCondition;

impl Cop for ParenthesesAroundCondition {
    fn name(&self) -> &'static str {
        "Style/ParenthesesAroundCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_ternary() {
            return;
        }
        let Some(cond) = node.condition() else {
            return;
        };
        // control_op_condition: (begin $_ $...)
        if !cond.is(NodeType::Begin) {
            return;
        }
        let Some(first_child) = cond.child(0) else {
            return;
        };
        let rest: Vec<Node<'a>> = cond.child_nodes().skip(1).collect();
        if require_parentheses(node, first_child)
            || semicolon_separated_expressions(cx, first_child, &rest)
            || modifier_op(first_child)
            || parens_allowed(cx, cond)
        {
            return;
        }
        let keyword = node.keyword();
        let article = if keyword == "while" { "a" } else { "an" };
        let message =
            format!("Don't use parentheses around the condition of {article} `{keyword}`.");
        cx.add_offense_with_correction(cond.source_range(), &message, |corrector| {
            parentheses_corrector::correct(corrector, cond);
        });
    }
}

fn require_parentheses(node: Node<'_>, condition_body: Node<'_>) -> bool {
    if !matches!(node.ty(), NodeType::While | NodeType::Until) {
        return false;
    }
    if !condition_body.is_any_block() {
        return false;
    }
    condition_body
        .send_node()
        .is_some_and(|s| s.is_block_literal())
        && condition_body.has_keywords()
}

fn semicolon_separated_expressions(
    cx: &Investigation<'_, '_>,
    first: Node<'_>,
    rest: &[Node<'_>],
) -> bool {
    let Some(second) = rest.first() else {
        return false;
    };
    let range = crate::processed_source::Range::new(
        first.source_range().end_pos,
        second.source_range().begin_pos,
    );
    cx.source(range).contains(';')
}

fn modifier_op(node: Node<'_>) -> bool {
    if node.is(NodeType::If) && node.is_ternary() {
        return false;
    }
    if node.is(NodeType::Rescue) {
        return true;
    }
    node.is_basic_conditional() && node.is_modifier_form()
}

fn parens_allowed(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    parens_required(node)
        || (safe_assignment(node) && safe_assignment_kept(cx))
        || (node.is_multiline() && cx.cop_config_bool("AllowInMultilineConditions", false))
}

fn safe_assignment_kept(cx: &Investigation<'_, '_>) -> bool {
    if safe_assignment_allowed(cx) {
        return true;
    }
    let name = "Lint/AssignmentInCondition";
    cx.config.cop_enabled(name)
        && cx.config.for_cop(name).get("AllowSafeAssignment") == Some(&Value::Bool(true))
}
