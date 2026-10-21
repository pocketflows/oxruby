//! Port of `lib/rubocop/cop/lint/each_with_object_argument.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "The argument to each_with_object cannot be immutable.";

static EACH_WITH_OBJECT: NodePattern = NodePattern::new("(call _ :each_with_object $_)");

#[derive(Default)]
pub struct EachWithObjectArgument;

impl Cop for EachWithObjectArgument {
    fn name(&self) -> &'static str {
        "Lint/EachWithObjectArgument"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["each_with_object"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = EACH_WITH_OBJECT.match_node(node) else {
            return;
        };
        if captures[0].node().is_some_and(|arg| arg.is_immutable_literal()) {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}
