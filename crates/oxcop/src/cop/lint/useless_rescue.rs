//! Port of `lib/rubocop/cop/lint/useless_rescue.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Useless `rescue` detected.";

#[derive(Default)]
pub struct UselessRescue;

impl Cop for UselessRescue {
    fn name(&self) -> &'static str {
        "Lint/UselessRescue"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Rescue]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(resbody_node) = node.resbody_branches().last().copied() else {
            return;
        };
        if only_reraising(resbody_node) {
            cx.add_offense(resbody_node.source_range(), MSG);
        }
    }
}

fn only_reraising(resbody_node: Node<'_>) -> bool {
    if use_exception_variable_in_ensure(resbody_node) {
        return false;
    }
    let Some(body) = resbody_node.body() else {
        return false;
    };
    if !body.is(NodeType::Send) || !body.is_method("raise") || body.receiver().is_some() {
        return false;
    }
    if !body.has_arguments() {
        return true;
    }
    if body.arguments().len() > 1 {
        return false;
    }

    let Some(first) = body.first_argument() else {
        return false;
    };
    let exception_name = first.source();
    exception_objects(resbody_node).contains(&Some(exception_name))
}

fn use_exception_variable_in_ensure(resbody_node: Node<'_>) -> bool {
    let Some(exception_variable) = resbody_node.exception_variable() else {
        return false;
    };
    let Some(ensure_node) = resbody_node.ancestors().find(|a| a.is(NodeType::Ensure)) else {
        return false;
    };
    let Some(ensure_body) = ensure_node.ensure_branch() else {
        return false;
    };

    ensure_body
        .descendants()
        .filter(|d| d.is(NodeType::Lvar))
        .any(|d| d.source() == exception_variable.source())
}

fn exception_objects(resbody_node: Node<'_>) -> [Option<&str>; 3] {
    [
        resbody_node.exception_variable().map(|v| v.source()),
        Some("$!"),
        Some("$ERROR_INFO"),
    ]
}
