//! Port of `lib/rubocop/cop/style/missing_respond_to_missing.rb`.
//!
//! The project index (`rubydex`) isn't supported, so
//! `respond_to_missing_elsewhere?` is always false, as in RuboCop without
//! one.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "When using `method_missing`, define `respond_to_missing?`.";

#[derive(Default)]
pub struct MissingRespondToMissing;

impl Cop for MissingRespondToMissing {
    fn name(&self) -> &'static str {
        "Style/MissingRespondToMissing"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_method("method_missing") || implements_respond_to_missing(node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn implements_respond_to_missing(node: Node<'_>) -> bool {
    let scope = enclosing_scope(node);
    let Some(search_root) = scope.or_else(|| node.parent()) else {
        return false;
    };
    search_root.descendants().any(|descendant| {
        descendant.ty() == node.ty()
            && descendant.is_method("respond_to_missing?")
            && enclosing_scope(descendant) == scope
    })
}

/// The class/module/`class << self` body that lexically contains `node`, or
/// `None` at the top level.
fn enclosing_scope(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors()
        .find(|a| a.is_any(&[NodeType::Class, NodeType::Module, NodeType::Sclass]))
}
