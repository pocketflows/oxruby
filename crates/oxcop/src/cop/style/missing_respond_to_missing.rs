//! Port of `lib/rubocop/cop/style/missing_respond_to_missing.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::project_index_help::{indexed_singleton_of, resolve_constant_in_index};
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
        if !node.is_method("method_missing")
            || implements_respond_to_missing(node)
            || respond_to_missing_elsewhere(cx, node)
        {
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

/// `respond_to_missing_elsewhere?`: with the project index,
/// `respond_to_missing?` defined in another definition of the same class or
/// module (e.g. a reopening in another file) also satisfies the check.
fn respond_to_missing_elsewhere(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(index) = cx.project_index() else {
        return false;
    };
    let Some(namespace_node) = node
        .ancestors()
        .find(|a| a.is(NodeType::Class) || a.is(NodeType::Module))
    else {
        return false;
    };
    let Some(declaration) = namespace_node
        .identifier()
        .and_then(|identifier| resolve_constant_in_index(&index, identifier))
        .filter(|declaration| declaration.is_namespace())
    else {
        return false;
    };
    let scope = if singleton_definition(node) {
        indexed_singleton_of(&index, declaration)
    } else {
        Some(declaration)
    };
    scope.is_some_and(|scope| index.member(scope, "respond_to_missing?()").is_some())
}

/// `singleton_definition?`.
fn singleton_definition(node: Node<'_>) -> bool {
    node.is(NodeType::Defs)
        || node
            .ancestors()
            .find(|a| a.is_any(&[NodeType::Sclass, NodeType::Class, NodeType::Module]))
            .is_some_and(|a| a.is(NodeType::Sclass))
}
