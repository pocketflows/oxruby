//! Port of `lib/rubocop/cop/style/auto_resource_cleanup.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static FILE_OPEN_METHOD: NodePattern =
    NodePattern::new("(send (const {nil? cbase} {:File :Tempfile}) :open ...)");

#[derive(Default)]
pub struct AutoResourceCleanup;

impl Cop for AutoResourceCleanup {
    fn name(&self) -> &'static str {
        "Style/AutoResourceCleanup"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["open"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !FILE_OPEN_METHOD.is_match(node) || cleanup(node) {
            return;
        }
        let range = Range::new(
            node.receiver().unwrap().source_range().begin_pos,
            node.selector().unwrap().end_pos,
        );
        let message = format!("Use the block version of `{}`.", cx.source(range));
        cx.add_offense(node.source_range(), &message);
    }
}

fn cleanup(node: Node<'_>) -> bool {
    if node.is_block_argument() {
        return true;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is(NodeType::Block) || !parent.is(NodeType::Lvasgn)
}
