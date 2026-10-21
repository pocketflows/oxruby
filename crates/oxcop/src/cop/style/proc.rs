//! Port of `lib/rubocop/cop/style/proc.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `proc` instead of `Proc.new`.";

static PROC_NEW: NodePattern =
    NodePattern::new("(any_block $(send (const {nil? cbase} :Proc) :new) ...)");

#[derive(Default)]
pub struct Proc;

impl Cop for Proc {
    fn name(&self) -> &'static str {
        "Style/Proc"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = PROC_NEW.match_node(node) else {
            return;
        };
        let block_method = captures[0].node().unwrap();
        cx.add_offense_with_correction(block_method.source_range(), MSG, |corrector| {
            corrector.replace(block_method.source_range(), "proc");
        });
    }
}
