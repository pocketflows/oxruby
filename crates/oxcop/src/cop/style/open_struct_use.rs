//! Port of `lib/rubocop/cop/style/open_struct_use.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid using `OpenStruct`; use `Struct`, `Hash`, a class or test doubles instead.";

static USES_OPEN_STRUCT: NodePattern = NodePattern::new("(const {nil? (cbase)} :OpenStruct)");

#[derive(Default)]
pub struct OpenStructUse;

impl Cop for OpenStructUse {
    fn name(&self) -> &'static str {
        "Style/OpenStructUse"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !USES_OPEN_STRUCT.is_match(node) || custom_class_or_module_definition(node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn custom_class_or_module_definition(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| matches!(p.ty(), NodeType::Class | NodeType::Module))
        && node.left_siblings().is_empty()
}
