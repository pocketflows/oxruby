//! Port of `lib/rubocop/cop/naming/accessor_method_name.rb`.
//!
//! The project index (`AllCops/UseProjectIndex`, `rubydex`) isn't
//! supported, so `overrides_inherited_method?` is always false, as in
//! RuboCop without an index.

use crate::ast::{Node, NodeType};
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
