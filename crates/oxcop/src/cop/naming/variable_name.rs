//! Port of `lib/rubocop/cop/naming/variable_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::configurable_naming::{
    allowed_identifier, forbidden_identifier, forbidden_pattern, naming_format, valid_name,
};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct VariableName;

impl Cop for VariableName {
    fn name(&self) -> &'static str {
        "Naming/VariableName"
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            Lvasgn, Ivasgn, Cvasgn, Arg, Optarg, Restarg, Kwoptarg, Kwarg, Kwrestarg, Blockarg,
            Lvar, Gvasgn,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(name) = node.name() else {
            return;
        };
        if node.is(NodeType::Gvasgn) {
            if forbidden_name(cx, name) {
                register_forbidden_name(cx, node, name);
            }
            return;
        }
        if allowed_identifier(cx, name) {
            return;
        }
        if forbidden_name(cx, name) {
            register_forbidden_name(cx, node, name);
            return;
        }
        let style = style(cx);
        if valid_name(node, name, naming_format(style)) || matches_allowed_pattern(cx, name) {
            return;
        }
        let message = format!("Use {style} for variable names.");
        let range = node.loc().name.unwrap_or_else(|| node.source_range());
        cx.add_offense(range, &message);
    }
}

fn forbidden_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    forbidden_identifier(cx, name) || forbidden_pattern(cx, name)
}

fn register_forbidden_name(cx: &mut Investigation<'_, '_>, node: Node<'_>, name: &str) {
    let message = format!("`{name}` is forbidden, use another name instead.");
    cx.add_offense(node.loc().name.unwrap(), &message);
}
