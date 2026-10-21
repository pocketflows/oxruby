//! Port of `lib/rubocop/cop/naming/block_parameter_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::uncommunicative_name::check;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct BlockParameterName;

impl Cop for BlockParameterName {
    fn name(&self) -> &'static str {
        "Naming/BlockParameterName"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.has_arguments() {
            return;
        }
        check(cx, node, &node.arguments());
    }
}
