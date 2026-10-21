//! Port of `lib/rubocop/cop/style/multiline_block_chain.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid multi-line chains of blocks.";

#[derive(Default)]
pub struct MultilineBlockChain;

impl Cop for MultilineBlockChain {
    fn name(&self) -> &'static str {
        "Style/MultilineBlockChain"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(send) = node.send_node() else {
            return;
        };
        for send_node in send.each_node().filter(|n| n.is_call()) {
            let Some(receiver) = send_node.receiver() else {
                continue;
            };
            if !(receiver.is_any_block() && receiver.is_multiline()) {
                continue;
            }
            let range = range_between(
                receiver.loc().end.unwrap().begin_pos,
                send.source_range().end_pos,
            );
            cx.add_offense(range, MSG);
            // Done. If there are more blocks in the chain, they will be found
            // by subsequent calls to on_block.
            break;
        }
    }
}
