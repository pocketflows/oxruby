//! Port of `lib/rubocop/cop/style/one_class_per_file.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not define multiple classes/modules at the top level in a single file.";

#[derive(Default)]
pub struct OneClassPerFile {
    top_level_definitions: usize,
}

impl Cop for OneClassPerFile {
    fn name(&self) -> &'static str {
        "Style/OneClassPerFile"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.top_level_definitions = 0;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !top_level_definition(node) || allowed_class(cx, node) {
            return;
        }
        self.top_level_definitions += 1;
        if self.top_level_definitions <= 1 {
            return;
        }
        cx.add_offense(
            range_between(
                node.source_range().begin_pos,
                node.loc().name.unwrap().end_pos,
            ),
            MSG,
        );
    }
}

fn top_level_definition(node: Node<'_>) -> bool {
    match node.parent() {
        Some(parent) if parent.is(NodeType::Begin) => parent.is_root(),
        _ => node.is_root(),
    }
}

fn allowed_class(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let Some(name) = node.identifier().and_then(|i| i.short_name()) else {
        return false;
    };
    match cx.cop_config_value("AllowedClasses") {
        Some(Value::Sequence(items)) => items.iter().any(|v| v.as_str() == Some(name)),
        _ => false,
    }
}
