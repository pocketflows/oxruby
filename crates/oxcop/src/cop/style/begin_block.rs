//! Port of `lib/rubocop/cop/style/begin_block.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid the use of `BEGIN` blocks.";

#[derive(Default)]
pub struct BeginBlock;

impl Cop for BeginBlock {
    fn name(&self) -> &'static str {
        "Style/BeginBlock"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Preexe]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        cx.add_offense(node.loc().keyword.unwrap(), MSG);
    }
}
