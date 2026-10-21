//! Port of `lib/rubocop/cop/lint/self_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Self-assignment detected.";

/// `ASSIGNMENT_TYPE_TO_RHS_TYPE`
fn assignment_type_to_rhs_type(ty: NodeType) -> Option<NodeType> {
    match ty {
        NodeType::Lvasgn => Some(NodeType::Lvar),
        NodeType::Ivasgn => Some(NodeType::Ivar),
        NodeType::Cvasgn => Some(NodeType::Cvar),
        NodeType::Gvasgn => Some(NodeType::Gvar),
        _ => None,
    }
}

#[derive(Default)]
pub struct SelfAssignment;

impl Cop for SelfAssignment {
    fn name(&self) -> &'static str {
        "Lint/SelfAssignment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Send,
            NodeType::Csend,
            NodeType::Lvasgn,
            NodeType::Ivasgn,
            NodeType::Cvasgn,
            NodeType::Gvasgn,
            NodeType::Casgn,
            NodeType::Masgn,
            NodeType::OrAsgn,
            NodeType::AndAsgn,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Send | NodeType::Csend => on_send(cx, node),
            NodeType::Lvasgn | NodeType::Ivasgn | NodeType::Cvasgn | NodeType::Gvasgn => {
                on_lvasgn(cx, node);
            }
            NodeType::Casgn => on_casgn(cx, node),
            NodeType::Masgn => on_masgn(cx, node),
            _ => on_or_asgn(cx, node),
        }
    }
}

fn on_send(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if allow_rbs_inline_annotation(cx) && rbs_inline_annotation(cx, node.receiver()) {
        return;
    }

    if node.is_method("[]=") {
        handle_key_assignment(cx, node);
    } else if node.is_assignment_method() && node.arguments().len() == 1 {
        handle_attribute_assignment(cx, node);
    }
}

fn on_lvasgn(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(rhs) = node.rhs() else {
        return;
    };
    if allow_rbs_inline_annotation(cx) && rbs_inline_annotation(cx, Some(rhs)) {
        return;
    }

    let rhs_type = assignment_type_to_rhs_type(node.ty());
    if Some(rhs.ty()) == rhs_type && Some(rhs.source()) == node.child_sym(0) {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn on_casgn(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(rhs) = node.rhs().filter(|r| r.is(NodeType::Const)) else {
        return;
    };
    if allow_rbs_inline_annotation(cx) && rbs_inline_annotation(cx, Some(rhs)) {
        return;
    }

    if same_option(node.namespace(), rhs.namespace()) && node.short_name() == rhs.short_name() {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn on_masgn(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let first_lhs = node.lhs().and_then(|lhs| mlhs_assignments(lhs).first().copied());
    if allow_rbs_inline_annotation(cx) && rbs_inline_annotation(cx, first_lhs) {
        return;
    }

    if multiple_self_assignment(node) {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn on_or_asgn(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if allow_rbs_inline_annotation(cx) && rbs_inline_annotation(cx, node.lhs()) {
        return;
    }

    let (Some(lhs), Some(rhs)) = (node.lhs(), node.rhs()) else {
        return;
    };
    if or_and_asgn_self_assignment(lhs, rhs) {
        cx.add_offense(node.source_range(), MSG);
    }
}

/// `MlhsNode#assignments`.
fn mlhs_assignments(node: Node<'_>) -> Vec<Node<'_>> {
    node.child_nodes()
        .flat_map(|child| {
            if child.is(NodeType::Splat) {
                child.child_nodes().collect()
            } else if child.is(NodeType::Mlhs) {
                mlhs_assignments(child)
            } else {
                vec![child]
            }
        })
        .collect()
}

fn same_option(a: Option<Node<'_>>, b: Option<Node<'_>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b),
        _ => false,
    }
}

fn same_nodes(a: &[Node<'_>], b: &[Node<'_>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.same_as(*y))
}

fn or_and_asgn_self_assignment(lhs: Node<'_>, rhs: Node<'_>) -> bool {
    match lhs.ty() {
        NodeType::Casgn => {
            rhs.is(NodeType::Const)
                && same_option(lhs.namespace(), rhs.namespace())
                && lhs.short_name() == rhs.short_name()
        }
        NodeType::Send | NodeType::Csend => reader_self_assignment(lhs, rhs),
        _ => rhs_matches_lhs(rhs, lhs),
    }
}

fn reader_self_assignment(lhs: Node<'_>, rhs: Node<'_>) -> bool {
    if rhs.ty() != lhs.ty() {
        return false;
    }
    if lhs.method_name() != rhs.method_name() {
        return false;
    }
    if !same_option(lhs.receiver(), rhs.receiver()) {
        return false;
    }
    if !same_nodes(&lhs.arguments(), &rhs.arguments()) {
        return false;
    }

    !lhs.arguments().iter().any(|a| a.is_call())
}

fn multiple_self_assignment(node: Node<'_>) -> bool {
    let (Some(lhs), Some(rhs)) = (node.lhs(), node.rhs()) else {
        return false;
    };
    if !rhs.is(NodeType::Array) {
        return false;
    }
    if lhs.children().len() != rhs.children().len() {
        return false;
    }

    lhs.child_nodes()
        .zip(rhs.child_nodes())
        .all(|(lhs_item, rhs_item)| rhs_matches_lhs(rhs_item, lhs_item))
}

fn rhs_matches_lhs(rhs: Node<'_>, lhs: Node<'_>) -> bool {
    assignment_type_to_rhs_type(lhs.ty()) == Some(rhs.ty())
        && rhs.children().first() == lhs.children().first()
}

fn handle_key_assignment(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(value_node) = node.last_argument() else {
        return;
    };
    let arguments = node.arguments();
    let node_arguments = &arguments[..arguments.len() - 1];

    if matches!(
        value_node.ty(),
        NodeType::Send | NodeType::Csend | NodeType::Index
    ) && value_node.is_method("[]")
        && same_option(node.receiver(), value_node.receiver())
        && !node_arguments.iter().any(|a| a.is_call())
        && same_nodes(node_arguments, &value_node.arguments())
    {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn handle_attribute_assignment(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(first_argument) = node.first_argument() else {
        return;
    };
    if !first_argument.is_call() || first_argument.has_arguments() {
        return;
    }

    let method_name = node.method_name().unwrap_or_default();
    if same_option(node.receiver(), first_argument.receiver())
        && first_argument.method_name() == Some(method_name.strip_suffix('=').unwrap_or(method_name))
    {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn rbs_inline_annotation(cx: &Investigation<'_, '_>, node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    cx.processed_source
        .node_comments(node)
        .iter()
        .any(|comment| comment.text.starts_with("#:"))
}

fn allow_rbs_inline_annotation(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowRBSInlineAnnotation", false)
}
