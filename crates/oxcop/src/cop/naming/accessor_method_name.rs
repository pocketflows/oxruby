//! Port of `lib/rubocop/cop/naming/accessor_method_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::project_index_help::{
    indexed_singleton_of, inherited_index_member, resolve_constant_in_index,
};
use crate::cop::{Cop, Investigation};

const MSG_READER: &str = "Do not prefix reader method names with `get_`.";
const MSG_WRITER: &str = "Do not prefix writer method names with `set_`.";

#[derive(Default)]
pub struct AccessorMethodName;

impl Cop for AccessorMethodName {
    fn name(&self) -> &'static str {
        "Naming/AccessorMethodName"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !proper_attribute_name(node) {
            return;
        }
        let message = if bad_reader_name(node) {
            MSG_READER
        } else if bad_writer_name(node) {
            MSG_WRITER
        } else {
            return;
        };
        if overrides_inherited_method(cx, node) {
            return;
        }
        cx.add_offense(node.loc().name.unwrap(), message);
    }
}

fn method_name<'a>(node: Node<'a>) -> &'a str {
    node.method_name().unwrap_or("")
}

fn proper_attribute_name(node: Node<'_>) -> bool {
    !method_name(node).ends_with(['!', '?', '='])
}

fn bad_reader_name(node: Node<'_>) -> bool {
    method_name(node).starts_with("get_") && !node.has_arguments()
}

fn bad_writer_name(node: Node<'_>) -> bool {
    let arguments = node.arguments();
    method_name(node).starts_with("set_")
        && arguments.len() == 1
        && arguments[0].is(NodeType::Arg)
}

/// `overrides_inherited_method?`: when `AllCops/UseProjectIndex` is
/// enabled, methods that override a method defined by an ancestor elsewhere
/// in the project are not reported.
fn overrides_inherited_method(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
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
    let scope = if node.is(NodeType::Defs) {
        indexed_singleton_of(&index, declaration)
    } else {
        Some(declaration)
    };
    scope.is_some_and(|scope| {
        inherited_index_member(
            &index,
            scope,
            &format!("{}()", node.method_name().unwrap_or_default()),
        )
    })
}
