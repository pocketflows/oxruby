//! Port of `lib/rubocop/cop/lint/no_return_in_begin_end_blocks.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not `return` in `begin..end` blocks in assignment contexts.";

#[derive(Default)]
pub struct NoReturnInBeginEndBlocks;

impl Cop for NoReturnInBeginEndBlocks {
    fn name(&self) -> &'static str {
        "Lint/NoReturnInBeginEndBlocks"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Lvasgn,
            NodeType::Ivasgn,
            NodeType::Cvasgn,
            NodeType::Gvasgn,
            NodeType::Casgn,
            NodeType::OrAsgn,
            NodeType::OpAsgn,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for kwbegin_node in node.each_node().filter(|n| n.is(NodeType::Kwbegin)) {
            for return_node in kwbegin_node.each_node().filter(|n| n.is(NodeType::Return)) {
                if return_from_inner_scope(return_node, kwbegin_node) {
                    continue;
                }
                cx.add_offense(return_node.source_range(), MSG);
            }
        }
    }
}

fn return_from_inner_scope(return_node: Node<'_>, kwbegin_node: Node<'_>) -> bool {
    for ancestor in return_node.ancestors() {
        if ancestor == kwbegin_node {
            break;
        }
        if ancestor.is_any_def() || (ancestor.is_any_block() && ancestor.is_lambda()) {
            return true;
        }
    }
    false
}
