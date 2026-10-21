//! Port of `lib/rubocop/cop/mixin/safe_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;

/// `empty_condition?`: `(begin)`.
pub fn empty_condition(node: Node<'_>) -> bool {
    node.is(NodeType::Begin) && node.children().is_empty()
}

/// `setter_method?`: `[(call ...) setter_method?]`.
pub fn setter_method(node: Node<'_>) -> bool {
    node.is_call() && node.is_setter_method()
}

/// `safe_assignment?`: `(begin {equals_asgn? #setter_method?})`.
pub fn safe_assignment(node: Node<'_>) -> bool {
    node.is(NodeType::Begin)
        && node.children().len() == 1
        && node
            .child(0)
            .is_some_and(|c| c.is_equals_asgn() || setter_method(c))
}

pub fn safe_assignment_allowed(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowSafeAssignment", false)
}
