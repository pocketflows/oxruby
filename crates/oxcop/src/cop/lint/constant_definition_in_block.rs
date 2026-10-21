//! Port of `lib/rubocop/cop/lint/constant_definition_in_block.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not define constants this way within a block.";

static CONSTANT_ASSIGNED_IN_BLOCK: NodePattern =
    NodePattern::new("({^any_block [^begin ^^any_block]} nil? ...)");
static MODULE_DEFINED_IN_BLOCK: NodePattern =
    NodePattern::new("({^any_block [^begin ^^any_block]} ...)");

#[derive(Default)]
pub struct ConstantDefinitionInBlock;

impl Cop for ConstantDefinitionInBlock {
    fn name(&self) -> &'static str {
        "Lint/ConstantDefinitionInBlock"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Casgn, NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let pattern = if node.is(NodeType::Casgn) {
            &CONSTANT_ASSIGNED_IN_BLOCK
        } else {
            &MODULE_DEFINED_IN_BLOCK
        };
        if !pattern.is_match(node) || allowed_method(cx, method_name(node).unwrap_or_default()) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn method_name<'a>(node: Node<'a>) -> Option<&'a str> {
    node.ancestors()
        .find(|a| a.is_any_block())
        .and_then(|block| block.method_name())
}
