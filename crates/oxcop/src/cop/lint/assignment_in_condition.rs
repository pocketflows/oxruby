//! Port of `lib/rubocop/cop/lint/assignment_in_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::safe_assignment::{empty_condition, safe_assignment, safe_assignment_allowed};
use crate::cop::{Cop, Investigation};

const MSG_WITH_SAFE_ASSIGNMENT_ALLOWED: &str = "Use `==` if you meant to do a comparison or wrap the expression in parentheses to indicate you meant to assign in a condition.";
const MSG_WITHOUT_SAFE_ASSIGNMENT_ALLOWED: &str =
    "Use `==` if you meant to do a comparison or move the assignment up out of the condition.";

#[derive(Default)]
pub struct AssignmentInCondition;

impl Cop for AssignmentInCondition {
    fn name(&self) -> &'static str {
        "Lint/AssignmentInCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::While, NodeType::Until]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(condition) = node.condition() else {
            return;
        };
        traverse_node(cx, condition);
    }
}

fn is_asgn_type(node: Node<'_>) -> bool {
    node.is(NodeType::Begin) || node.is_equals_asgn() || node.is_call()
}

fn traverse_node<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.is_any_block() || node.is(NodeType::Defined) {
        return;
    }
    if is_asgn_type(node) {
        if skip_children(cx, node) {
            return;
        }
        if !allowed_construct(node) {
            let allowed = safe_assignment_allowed(cx);
            let message = if allowed {
                MSG_WITH_SAFE_ASSIGNMENT_ALLOWED
            } else {
                MSG_WITHOUT_SAFE_ASSIGNMENT_ALLOWED
            };
            cx.add_offense_with_correction(node.loc().operator.unwrap(), message, |corrector| {
                if allowed {
                    corrector.wrap(node.source_range(), "(", ")");
                }
            });
        }
    }
    for child in node.child_nodes() {
        traverse_node(cx, child);
    }
}

fn allowed_construct(node: Node<'_>) -> bool {
    if node.is(NodeType::Begin) {
        return true;
    }
    conditional_assignment(node) || discarded_assignment(node)
}

fn conditional_assignment(node: Node<'_>) -> bool {
    node.loc().operator.is_none()
}

fn discarded_assignment(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::Begin) && p.children().len() > 1)
}

fn skip_children(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    (node.is_call() && !node.is_assignment_method())
        || empty_condition(node)
        || (safe_assignment_allowed(cx) && safe_assignment(node))
}
