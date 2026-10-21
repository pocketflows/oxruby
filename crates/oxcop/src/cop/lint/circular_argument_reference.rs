//! Port of `lib/rubocop/cop/lint/circular_argument_reference.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct CircularArgumentReference;

fn message(arg_name: &str) -> String {
    format!("Circular argument reference - `{arg_name}`.")
}

impl Cop for CircularArgumentReference {
    fn name(&self) -> &'static str {
        "Lint/CircularArgumentReference"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Kwoptarg, NodeType::Optarg]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let (Some(arg_name), Some(arg_value)) = (node.child_sym(0), node.child(1)) else {
            return;
        };
        check_for_circular_argument_references(cx, arg_name, arg_value);
    }
}

fn check_for_circular_argument_references<'a>(
    cx: &mut Investigation<'a, '_>,
    arg_name: &str,
    arg_value: Node<'a>,
) {
    if arg_value.is(NodeType::Lvar) && arg_value.child_sym(0) == Some(arg_name) {
        cx.add_offense(arg_value.source_range(), &message(arg_name));
        return;
    }
    check_assignment_chain(cx, arg_name, arg_value);
}

fn check_assignment_chain<'a>(cx: &mut Investigation<'a, '_>, arg_name: &str, node: Node<'a>) {
    if !node.is(NodeType::Lvasgn) {
        return;
    }
    let mut seen_variables: Vec<&str> = Vec::new();
    let mut current_node = node;
    while current_node.is(NodeType::Lvasgn) {
        if let Some(name) = current_node.child_sym(0) {
            seen_variables.push(name);
        }
        match current_node.child_from_end(1) {
            Some(last) => current_node = last,
            None => return,
        }
    }
    if !current_node.is(NodeType::Lvar) {
        return;
    }
    let Some(variable_node) = current_node.child_sym(0) else {
        return;
    };
    if !(seen_variables.contains(&variable_node) || variable_node == arg_name) {
        return;
    }
    cx.add_offense(current_node.source_range(), &message(arg_name));
}
