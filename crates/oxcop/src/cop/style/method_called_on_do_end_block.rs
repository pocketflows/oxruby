//! Port of `lib/rubocop/cop/style/method_called_on_do_end_block.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid chaining a method call on a do...end block.";

#[derive(Default)]
pub struct MethodCalledOnDoEndBlock {
    ignored: IgnoredNodes,
}

impl Cop for MethodCalledOnDoEndBlock {
    fn name(&self) -> &'static str {
        "Style/MethodCalledOnDoEndBlock"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Send,
            NodeType::Csend,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        // `ignored_node?` compares by identity: nodes of an earlier source
        // never match.
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() {
            // If the method that is chained on the do...end block is itself a
            // method with a block, we allow it.
            if let Some(send_node) = node.send_node() {
                self.ignored.ignore(send_node);
            }
            return;
        }
        if self.ignored.contains(node) {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        if !(receiver.is_any_block() && receiver.has_keywords()) {
            return;
        }
        let range = range_between(
            receiver.loc().end.unwrap().begin_pos,
            node.source_range().end_pos,
        );
        cx.add_offense(range, MSG);
    }
}
