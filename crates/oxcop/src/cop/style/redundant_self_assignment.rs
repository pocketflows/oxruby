//! Port of `lib/rubocop/cop/style/redundant_self_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const METHODS_RETURNING_SELF: &[&str] = &[
    "append",
    "clear",
    "collect!",
    "compare_by_identity",
    "concat",
    "delete_if",
    "fill",
    "initialize_copy",
    "insert",
    "keep_if",
    "map!",
    "merge!",
    "prepend",
    "push",
    "rehash",
    "replace",
    "reverse!",
    "rotate!",
    "shuffle!",
    "sort!",
    "sort_by!",
    "transform_keys!",
    "transform_values!",
    "unshift",
    "update",
];

fn method_returning_self(name: Option<&str>) -> bool {
    name.is_some_and(|n| METHODS_RETURNING_SELF.contains(&n))
}

#[derive(Default)]
pub struct RedundantSelfAssignment;

impl Cop for RedundantSelfAssignment {
    fn name(&self) -> &'static str {
        "Style/RedundantSelfAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Lvasgn,
            NodeType::Ivasgn,
            NodeType::Cvasgn,
            NodeType::Gvasgn,
            NodeType::Send,
            NodeType::Csend,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_call() {
            self.on_send(cx, node);
            return;
        }
        let Some(rhs) = node.rhs() else {
            return;
        };
        if !(rhs.is_any_block() || rhs.is_call()) || !method_returning_self(rhs.method_name()) {
            return;
        }
        let Some(receiver) = rhs.receiver() else {
            return;
        };
        let receiver_type = match node.ty() {
            NodeType::Lvasgn => NodeType::Lvar,
            NodeType::Ivasgn => NodeType::Ivar,
            NodeType::Cvasgn => NodeType::Cvar,
            _ => NodeType::Gvar,
        };
        if !(receiver.is(receiver_type) && receiver.child_sym(0) == node.name()) {
            return;
        }
        let message = format!(
            "Redundant self assignment detected. Method `{}` modifies its receiver in place.",
            rhs.method_name().unwrap_or("")
        );
        cx.add_offense_with_correction(node.loc().operator.unwrap(), &message, |corrector| {
            corrector.replace(node.source_range(), rhs.source());
        });
    }
}

impl RedundantSelfAssignment {
    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_assignment_method() || !redundant_assignment(node) {
            return;
        }
        let first_argument = node.first_argument().unwrap();
        let message = format!(
            "Redundant self assignment detected. Method `{}` modifies its receiver in place.",
            first_argument.method_name().unwrap_or("")
        );
        let Some(operator) = node.loc().operator else {
            return;
        };
        cx.add_offense_with_correction(operator, &message, |corrector| {
            corrector.remove(range_between(
                node.source_range().begin_pos,
                first_argument.source_range().begin_pos,
            ));
        });
    }
}

/// `(call %1 _ (call (call %1 %2) #method_returning_self? ...))`
fn redundant_assignment(node: Node<'_>) -> bool {
    let Some(method) = node.method_name() else {
        return false;
    };
    let receiver_name = method.strip_suffix('=').unwrap_or(method);
    if node.children().len() != 3 {
        return false;
    }
    let Some(arg) = node.child(2) else {
        return false;
    };
    if !arg.is_call() || !method_returning_self(arg.method_name()) {
        return false;
    }
    let Some(inner) = arg.receiver() else {
        return false;
    };
    inner.is_call()
        && inner.children().len() == 2
        && inner.method_name() == Some(receiver_name)
        && match (inner.receiver(), node.receiver()) {
            (None, None) => true,
            (Some(a), Some(b)) => a.same_as(b),
            _ => false,
        }
}
