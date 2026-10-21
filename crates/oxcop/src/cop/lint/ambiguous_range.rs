//! Port of `lib/rubocop/cop/lint/ambiguous_range.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::rational_literal::rational_literal;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Wrap complex range boundaries with parentheses to avoid ambiguity.";

#[derive(Default)]
pub struct AmbiguousRange;

impl Cop for AmbiguousRange {
    fn name(&self) -> &'static str {
        "Lint/AmbiguousRange"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Irange, NodeType::Erange]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for boundary in each_boundary(node) {
            if acceptable(cx, boundary) {
                continue;
            }
            cx.add_offense_with_correction(boundary.source_range(), MSG, |corrector| {
                corrector.wrap(boundary.source_range(), "(", ")");
            });
        }
    }
}

fn each_boundary(range: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    [range.child(0), range.child(1)].into_iter().flatten()
}

fn acceptable(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is(NodeType::Begin)
        || node.is_literal()
        || rational_literal(node)
        || node.is_variable()
        || node.is(NodeType::Const)
        || node.is(NodeType::SelfNode)
        || (node.is_call() && acceptable_call(cx, node))
}

fn acceptable_call(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if node.is_unary_operation() {
        return true;
    }
    if node.receiver().is_some_and(|r| r.is_basic_literal()) {
        return false;
    }
    if node.is_operator_method() && !node.is_method("[]") {
        return false;
    }
    require_parentheses_for_method_chain(cx) || node.receiver().is_none()
}

fn require_parentheses_for_method_chain(cx: &Investigation<'_, '_>) -> bool {
    !cx.cop_config_bool("RequireParenthesesForMethodChains", false)
}
