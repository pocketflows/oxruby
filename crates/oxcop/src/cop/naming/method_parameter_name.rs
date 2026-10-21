//! Port of `lib/rubocop/cop/naming/method_parameter_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::uncommunicative_name::check;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct MethodParameterName;

impl Cop for MethodParameterName {
    fn name(&self) -> &'static str {
        "Naming/MethodParameterName"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.has_arguments() {
            return;
        }
        check(cx, node, &node.arguments());
    }
}
