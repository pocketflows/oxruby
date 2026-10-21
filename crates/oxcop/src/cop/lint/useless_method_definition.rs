//! Port of `lib/rubocop/cop/lint/useless_method_definition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Useless method definition detected.";

#[derive(Default)]
pub struct UselessMethodDefinition;

impl Cop for UselessMethodDefinition {
    fn name(&self) -> &'static str {
        "Lint/UselessMethodDefinition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if method_definition_with_modifier(node) || use_rest_or_optional_args(node) {
            return;
        }
        if !delegating(node.body(), node) {
            return;
        }

        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let range = match node.parent() {
                Some(parent) if parent.is(NodeType::Send) => parent,
                _ => node,
            };
            corrector.remove(range.source_range());
        });
    }
}

fn method_definition_with_modifier(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::Send) && !p.is_non_bare_access_modifier())
}

fn use_rest_or_optional_args(node: Node<'_>) -> bool {
    node.arguments().iter().any(|arg| {
        matches!(
            arg.ty(),
            NodeType::Restarg | NodeType::Optarg | NodeType::Kwoptarg | NodeType::Kwrestarg
        )
    })
}

fn delegating(node: Option<Node<'_>>, def_node: Node<'_>) -> bool {
    match node {
        Some(node) if node.is(NodeType::Zsuper) => true,
        Some(node) if node.is(NodeType::Super) => {
            let a: Vec<&str> = node.arguments().iter().map(|a| a.source()).collect();
            let b: Vec<&str> = def_node.arguments().iter().map(|a| a.source()).collect();
            a == b
        }
        _ => false,
    }
}
