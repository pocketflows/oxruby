//! Port of `lib/rubocop/cop/style/optional_boolean_parameter.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct OptionalBooleanParameter;

impl Cop for OptionalBooleanParameter {
    fn name(&self) -> &'static str {
        "Style/OptionalBooleanParameter"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if allowed_method(cx, node.method_name().unwrap_or("")) {
            return;
        }
        for arg in node.arguments() {
            if !arg.is(NodeType::Optarg) {
                continue;
            }
            let Some(default_value) = arg.child(1) else {
                continue;
            };
            if default_value.is_boolean() {
                let message = format!(
                    "Prefer keyword arguments for arguments with a boolean default value; use `{}: {}` instead of `{}`.",
                    arg.name().unwrap_or(""),
                    default_value.source(),
                    arg.source()
                );
                cx.add_offense(arg.source_range(), &message);
            }
        }
    }
}
