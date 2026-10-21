//! Port of `lib/rubocop/cop/naming/constant_name.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Use SCREAMING_SNAKE_CASE for constants.";

static CLASS_OR_STRUCT_RETURN_METHOD: NodePattern =
    NodePattern::new("(send (const _ {:Class :Struct}) :new ...)");
static LITERAL_RECEIVER: NodePattern =
    NodePattern::new("{(send literal? ...) (send (begin literal?) ...)}");

#[derive(Default)]
pub struct ConstantName;

impl Cop for ConstantName {
    fn name(&self) -> &'static str {
        "Naming/ConstantName"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Casgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let value = match node.parent() {
            Some(parent) if parent.is(NodeType::OrAsgn) => parent.expression(),
            _ => node.expression(),
        };
        if allowed_assignment(value) {
            return;
        }
        if regex!(r"^[[:digit:][:upper:]_]+$")
            .find(node.name().unwrap_or(""))
            .is_some()
        {
            return;
        }
        cx.add_offense(node.loc().name.unwrap(), MSG);
    }
}

fn allowed_assignment(value: Option<Node<'_>>) -> bool {
    let Some(value) = value else {
        return false;
    };
    matches!(value.ty(), NodeType::Block | NodeType::Const | NodeType::Casgn)
        || (value.is_send() && (value.receiver().is_none() || !LITERAL_RECEIVER.is_match(value)))
        || CLASS_OR_STRUCT_RETURN_METHOD.is_match(value)
        || (value.is(NodeType::If)
            && value
                .branches()
                .into_iter()
                .flatten()
                .any(|b| b.is(NodeType::Const)))
}
