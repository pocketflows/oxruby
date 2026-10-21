//! Port of `lib/rubocop/cop/style/end_block.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid the use of `END` blocks. Use `Kernel#at_exit` instead.";

#[derive(Default)]
pub struct EndBlock;

impl Cop for EndBlock {
    fn name(&self) -> &'static str {
        "Style/EndBlock"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Postexe]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let keyword = node.loc().keyword.unwrap();
        cx.add_offense_with_correction(keyword, MSG, |corrector| {
            corrector.replace(keyword, "at_exit");
        });
    }
}
