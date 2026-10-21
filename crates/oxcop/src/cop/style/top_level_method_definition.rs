//! Port of `lib/rubocop/cop/style/top_level_method_definition.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not define methods at the top-level.";

static DEFINE_METHOD_BLOCK: NodePattern =
    NodePattern::new("(any_block (send _ :define_method _) ...)");

#[derive(Default)]
pub struct TopLevelMethodDefinition;

impl Cop for TopLevelMethodDefinition {
    fn name(&self) -> &'static str {
        "Style/TopLevelMethodDefinition"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Def,
            NodeType::Defs,
            NodeType::Send,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["define_method"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() && !DEFINE_METHOD_BLOCK.is_match(node) {
            return;
        }
        if !top_level_method_definition(node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn top_level_method_definition(node: Node<'_>) -> bool {
    match node.parent() {
        Some(parent) if parent.is(NodeType::Begin) => parent.is_root(),
        _ => node.is_root(),
    }
}
