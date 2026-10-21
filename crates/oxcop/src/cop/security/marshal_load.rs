//! Port of `lib/rubocop/cop/security/marshal_load.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static MARSHAL_LOAD: NodePattern = NodePattern::new(
    "(send (const {nil? cbase} :Marshal) ${:load :restore}
       !(send (const {nil? cbase} :Marshal) :dump ...) _?)",
);

#[derive(Default)]
pub struct MarshalLoad;

impl Cop for MarshalLoad {
    fn name(&self) -> &'static str {
        "Security/MarshalLoad"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["load", "restore"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = MARSHAL_LOAD.match_node(node) else {
            return;
        };
        let method = captures[0].sym().unwrap_or("");
        let message = format!("Avoid using `Marshal.{method}`.");
        cx.add_offense(node.loc().selector.unwrap(), &message);
    }
}
