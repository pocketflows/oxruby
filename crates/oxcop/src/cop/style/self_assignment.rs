//! Port of `lib/rubocop/cop/style/self_assignment.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};

const OPS: &[&str] = &["+", "-", "*", "**", "/", "%", "^", "<<", ">>", "|", "&"];

#[derive(Default)]
pub struct SelfAssignment;

impl Cop for SelfAssignment {
    fn name(&self) -> &'static str {
        "Style/SelfAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Layout/SpaceAroundOperators"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Lvasgn, NodeType::Ivasgn, NodeType::Cvasgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let var_type = match node.ty() {
            NodeType::Lvasgn => NodeType::Lvar,
            NodeType::Ivasgn => NodeType::Ivar,
            _ => NodeType::Cvar,
        };
        check(cx, node, var_type);
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>, var_type: NodeType) {
    let Some(rhs) = node.expression() else {
        return;
    };
    let var_name = node.name().unwrap_or_default();
    if rhs.is_send() && rhs.arguments().len() == 1 {
        check_send_node(cx, node, rhs, var_name, var_type);
    } else if rhs.is_operator_keyword() {
        check_boolean_node(cx, node, rhs, var_name, var_type);
    }
}

/// `node == s(var_type, var_name)`.
fn is_target(node: Option<Node<'_>>, var_type: NodeType, var_name: &str) -> bool {
    node.is_some_and(|n| {
        n.is(var_type) && n.children() == [Child::Sym(var_name.to_owned())].as_slice()
    })
}

fn check_send_node(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    rhs: Node<'_>,
    var_name: &str,
    var_type: NodeType,
) {
    let method = rhs.method_name().unwrap_or_default();
    if !OPS.contains(&method) || !is_target(rhs.receiver(), var_type, var_name) {
        return;
    }
    let message = format!("Use self-assignment shorthand `{method}=`.");
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        autocorrect(corrector, node);
    });
}

fn check_boolean_node(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    rhs: Node<'_>,
    var_name: &str,
    var_type: NodeType,
) {
    if !is_target(rhs.lhs(), var_type, var_name) {
        return;
    }
    let message = format!(
        "Use self-assignment shorthand `{}=`.",
        rhs.operator().unwrap_or_default()
    );
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        autocorrect(corrector, node);
    });
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    let rhs = node.expression().unwrap();
    if rhs.is_send() {
        apply_autocorrect(
            corrector,
            node,
            rhs,
            rhs.method_name().unwrap(),
            rhs.first_argument().unwrap(),
        );
    } else if rhs.is_operator_keyword() {
        apply_autocorrect(
            corrector,
            node,
            rhs,
            rhs.operator().unwrap(),
            rhs.rhs().unwrap(),
        );
    }
}

fn apply_autocorrect(
    corrector: &mut Corrector,
    node: Node<'_>,
    rhs: Node<'_>,
    operator: &str,
    new_rhs: Node<'_>,
) {
    corrector.insert_before(node.loc().operator.unwrap(), operator);
    corrector.replace(rhs.source_range(), new_rhs.source());
}
