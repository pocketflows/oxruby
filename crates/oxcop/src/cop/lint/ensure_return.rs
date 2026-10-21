//! Port of `lib/rubocop/cop/lint/ensure_return.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not return from an `ensure` block.";

#[derive(Default)]
pub struct EnsureReturn;

impl Cop for EnsureReturn {
    fn name(&self) -> &'static str {
        "Lint/EnsureReturn"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Ensure]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(branch) = node.ensure_branch() else {
            return;
        };
        for return_node in branch.each_node().filter(|n| n.is(NodeType::Return)) {
            if return_from_inner_scope(return_node, node) {
                continue;
            }
            cx.add_offense(return_node.source_range(), MSG);
        }
    }
}

fn return_from_inner_scope(return_node: Node<'_>, ensure_node: Node<'_>) -> bool {
    for ancestor in return_node.ancestors() {
        if ancestor == ensure_node {
            break;
        }
        if ancestor.is_any_def() || (ancestor.is_any_block() && ancestor.is_lambda()) {
            return true;
        }
    }
    false
}
