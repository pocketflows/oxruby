//! Port of `lib/rubocop/cop/metrics/module_length.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::code_length::check_code_length;
use crate::cop::{Cop, Investigation};

const LABEL: &str = "Module";

static MODULE_DEFINITION: NodePattern =
    NodePattern::new("(casgn nil? _ (any_block (send (const {nil? cbase} :Module) :new) ...))");

#[derive(Default)]
pub struct ModuleLength;

impl Cop for ModuleLength {
    fn name(&self) -> &'static str {
        "Metrics/ModuleLength"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Module, NodeType::Casgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Casgn) && !MODULE_DEFINITION.is_match(node) {
            return;
        }
        check_code_length(cx, node, LABEL);
    }
}
