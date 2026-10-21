//! Port of `lib/rubocop/cop/lint/next_without_accumulator.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `next` with an accumulator argument in a `reduce`.";

static ON_BLOCK_BODY_OF_REDUCE: NodePattern = NodePattern::new(
    "{
      (block (call _recv {:reduce :inject} !sym) _blockargs $(begin ...))
      (numblock (call _recv {:reduce :inject} !sym) _argscount $(begin ...))
      (itblock (call _recv {:reduce :inject} !sym) _argscount $(begin ...))
    }",
);

#[derive(Default)]
pub struct NextWithoutAccumulator;

impl Cop for NextWithoutAccumulator {
    fn name(&self) -> &'static str {
        "Lint/NextWithoutAccumulator"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = ON_BLOCK_BODY_OF_REDUCE
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };
        let void_next = body.each_node().find(|n| {
            n.is(NodeType::Next)
                && n.children().is_empty()
                && parent_block_node(*n).is_some_and(|p| p.same_as(node))
        });
        if let Some(void_next) = void_next {
            cx.add_offense(void_next.source_range(), MSG);
        }
    }
}

fn parent_block_node(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors().find(|a| a.is_any_block())
}
