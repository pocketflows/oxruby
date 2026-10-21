//! Port of `lib/rubocop/cop/style/constant_visibility.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static VISIBILITY_DECLARATION_FOR: NodePattern =
    NodePattern::new("(send nil? {:public_constant :private_constant} $...)");

#[derive(Default)]
pub struct ConstantVisibility;

impl Cop for ConstantVisibility {
    fn name(&self) -> &'static str {
        "Style/ConstantVisibility"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Casgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !class_or_module_scope(node) || visibility_declaration(node) {
            return;
        }
        if cx.cop_config_bool("IgnoreModules", false) && module(node) {
            return;
        }
        let message = format!(
            "Explicitly make `{}` public or private using either `#public_constant` or `#private_constant`.",
            node.name().unwrap_or("")
        );
        cx.add_offense(node.source_range(), &message);
    }
}

fn module(node: Node<'_>) -> bool {
    node.expression().is_some_and(|e| e.is_class_constructor())
}

fn class_or_module_scope(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.ty() {
        NodeType::Begin => class_or_module_scope(parent),
        NodeType::Class | NodeType::Module => true,
        _ => false,
    }
}

fn visibility_declaration(node: Node<'_>) -> bool {
    let name = node.name();
    node.parent().unwrap().child_nodes().any(|child| {
        if !child.is_send() {
            return false;
        }
        let Some(captures) = VISIBILITY_DECLARATION_FOR.match_node(child) else {
            return false;
        };
        let mut arguments = captures[0].nodes();
        if let Some(splat) = arguments.first().copied().filter(|a| a.is(NodeType::Splat)) {
            let Some(array) = splat.child(0).filter(|c| c.is(NodeType::Array)) else {
                return true;
            };
            arguments = array.child_nodes().collect();
        }
        arguments.iter().any(|argument| {
            let value = match argument.ty() {
                NodeType::Sym => argument.sym_value(),
                NodeType::Str => argument.str_content(),
                _ => None,
            };
            value.is_some() && value == name
        })
    })
}
