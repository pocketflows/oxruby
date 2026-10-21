//! Port of `lib/rubocop/cop/lint/useless_setter_call.rb`.

use std::collections::HashMap;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::indent;
use crate::cop::{Cop, Investigation};

const ASSIGNMENT_TYPES: &[NodeType] = &[
    NodeType::Lvasgn,
    NodeType::Ivasgn,
    NodeType::Cvasgn,
    NodeType::Gvasgn,
];

#[derive(Default)]
pub struct UselessSetterCall;

impl Cop for UselessSetterCall {
    fn name(&self) -> &'static str {
        "Lint/UselessSetterCall"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = node.body() else {
            return;
        };
        let Some(last_expr) = last_expression(body) else {
            return;
        };
        if !setter_call_to_local_variable(last_expr) {
            return;
        }

        let mut tracker = MethodVariableTracker::new(body);
        let Some(receiver) = last_expr.child(0) else {
            return;
        };
        let Some(variable_name) = receiver.child_sym(0) else {
            return;
        };
        if !tracker.contain_local_object(variable_name) {
            return;
        }

        let Some(loc_name) = receiver.loc().name else {
            return;
        };
        let name = cx.source(loc_name);
        let message = format!("Useless setter call to local variable `{name}`.");
        cx.add_offense_with_correction(loc_name, &message, |corrector| {
            corrector.insert_after(
                last_expr.source_range(),
                format!("\n{}{name}", indent(last_expr, 0)),
            );
        });
    }
}

/// `[(send (lvar _) ...) setter_method?]`
fn setter_call_to_local_variable(node: Node<'_>) -> bool {
    node.is(NodeType::Send)
        && node.child(0).is_some_and(|r| r.is(NodeType::Lvar))
        && node.is_setter_method()
}

fn last_expression(body: Node<'_>) -> Option<Node<'_>> {
    if body.is(NodeType::Begin) {
        body.child_nodes().next_back()
    } else {
        Some(body)
    }
}

struct MethodVariableTracker<'a> {
    body_node: Node<'a>,
    local: Option<HashMap<&'a str, bool>>,
}

impl<'a> MethodVariableTracker<'a> {
    fn new(body_node: Node<'a>) -> Self {
        Self {
            body_node,
            local: None,
        }
    }

    fn contain_local_object(&mut self, variable_name: &str) -> bool {
        if self.local.is_none() {
            let mut local = HashMap::new();
            scan(self.body_node, &mut local);
            self.local = Some(local);
        }
        self.local
            .as_ref()
            .and_then(|l| l.get(variable_name).copied())
            .unwrap_or(false)
    }
}

fn scan<'a>(node: Node<'a>, local: &mut HashMap<&'a str, bool>) {
    let skip_children = process_assignment_node(node, local);
    if skip_children {
        return;
    }
    for child_node in node.child_nodes() {
        scan(child_node, local);
    }
}

/// Returns whether to skip the children (`throw :skip_children`).
fn process_assignment_node<'a>(node: Node<'a>, local: &mut HashMap<&'a str, bool>) -> bool {
    match node.ty() {
        NodeType::Masgn => process_multiple_assignment(node, local),
        NodeType::OrAsgn | NodeType::AndAsgn => process_logical_operator_assignment(node, local),
        NodeType::OpAsgn => process_binary_operator_assignment(node, local),
        ty if ASSIGNMENT_TYPES.contains(&ty) => {
            if let Some(rhs) = node.rhs() {
                process_assignment(node, rhs, local);
            }
            false
        }
        _ => false,
    }
}

fn process_multiple_assignment<'a>(masgn_node: Node<'a>, local: &mut HashMap<&'a str, bool>) -> bool {
    let (Some(lhs), Some(rhs)) = (masgn_node.lhs(), masgn_node.rhs()) else {
        return true;
    };
    for (index, lhs_node) in lhs.child_nodes().enumerate() {
        if !ASSIGNMENT_TYPES.contains(&lhs_node.ty()) {
            continue;
        }
        let rhs_node = if rhs.is(NodeType::Array) {
            rhs.child(index)
        } else {
            None
        };
        if let Some(rhs_node) = rhs_node {
            process_assignment(lhs_node, rhs_node, local);
        } else if let Some(name) = lhs_node.name() {
            local.insert(name, true);
        }
    }
    true
}

fn process_logical_operator_assignment<'a>(
    asgn_node: Node<'a>,
    local: &mut HashMap<&'a str, bool>,
) -> bool {
    let (Some(lhs), Some(rhs)) = (asgn_node.lhs(), asgn_node.rhs()) else {
        return false;
    };
    if !ASSIGNMENT_TYPES.contains(&lhs.ty()) {
        return false;
    }
    process_assignment(lhs, rhs, local);
    true
}

fn process_binary_operator_assignment<'a>(
    op_asgn_node: Node<'a>,
    local: &mut HashMap<&'a str, bool>,
) -> bool {
    let Some(lhs_node) = op_asgn_node.lhs() else {
        return false;
    };
    if !ASSIGNMENT_TYPES.contains(&lhs_node.ty()) {
        return false;
    }
    if let Some(name) = lhs_node.name() {
        local.insert(name, true);
    }
    true
}

fn process_assignment<'a>(
    asgn_node: Node<'a>,
    rhs_node: Node<'a>,
    local: &mut HashMap<&'a str, bool>,
) {
    let Some(name) = asgn_node.name() else {
        return;
    };
    let value = if rhs_node.is_variable() {
        rhs_node
            .name()
            .and_then(|n| local.get(n).copied())
            .unwrap_or(false)
    } else {
        constructor(rhs_node)
    };
    local.insert(name, value);
}

fn constructor(node: Node<'_>) -> bool {
    if node.is_literal() {
        return true;
    }
    if !node.is(NodeType::Send) {
        return false;
    }
    node.is_method("new")
}
