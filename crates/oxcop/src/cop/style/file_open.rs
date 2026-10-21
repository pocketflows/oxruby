//! Port of `lib/rubocop/cop/style/file_open.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "`File.open` without a block may leak a file descriptor; use the block form.";

static FILE_OPEN: NodePattern = NodePattern::new("(send (const {nil? cbase} :File) :open ...)");

#[derive(Default)]
pub struct FileOpen;

impl Cop for FileOpen {
    fn name(&self) -> &'static str {
        "Style/FileOpen"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["open"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !FILE_OPEN.is_match(node) || node.is_block_argument() || !offensive_usage(node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn offensive_usage(node: Node<'_>) -> bool {
    if !node.is_value_used() {
        return true;
    }
    let parent = node.parent().unwrap();
    parent.is(NodeType::Lvasgn) || parent.receiver() == Some(node)
}
