//! Port of `lib/rubocop/cop/metrics/class_length.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::code_length::check_code_length;
use crate::cop::{Cop, Investigation};

const LABEL: &str = "Class";

#[derive(Default)]
pub struct ClassLength;

impl Cop for ClassLength {
    fn name(&self) -> &'static str {
        "Metrics/ClassLength"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass, NodeType::Casgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Class => check_code_length(cx, node, LABEL),
            NodeType::Sclass => {
                if node.ancestors().any(|a| a.is(NodeType::Class)) {
                    return;
                }
                check_code_length(cx, node, LABEL);
            }
            _ => {
                let Some(block_node) = node
                    .expression()
                    .or_else(|| find_expression_within_parent(node.parent()))
                else {
                    return;
                };
                if !class_definition(block_node) {
                    return;
                }
                check_code_length(cx, block_node, LABEL);
            }
        }
    }
}

/// rubocop-ast's `class_definition?`.
fn class_definition(node: Node<'_>) -> bool {
    match node.ty() {
        NodeType::Class | NodeType::Sclass => true,
        _ if node.is_any_block() => node.send_node().is_some_and(|send| {
            send.is_send()
                && send.is_method("new")
                && send
                    .receiver()
                    .is_some_and(|r| r.is_global_const("Struct") || r.is_global_const("Class"))
        }),
        _ => false,
    }
}

fn find_expression_within_parent(parent: Option<Node<'_>>) -> Option<Node<'_>> {
    let parent = parent?;
    if parent.is_assignment() {
        parent.expression()
    } else if parent.parent().is_some_and(|p| p.is(NodeType::Masgn)) {
        parent.parent().and_then(|p| p.expression())
    } else {
        None
    }
}
