//! Port of `lib/rubocop/cop/security/json_load.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static INSECURE_JSON_LOAD: NodePattern = NodePattern::new(
    "(
       send (const {nil? cbase} :JSON) ${:load :restore}
       ...
       !`(pair (sym :create_additions) _)
     )",
);

#[derive(Default)]
pub struct JsonLoad;

impl Cop for JsonLoad {
    fn name(&self) -> &'static str {
        "Security/JSONLoad"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["load", "restore"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = INSECURE_JSON_LOAD.match_node(node) else {
            return;
        };
        let method = captures[0].sym().unwrap_or("");
        let selector = node.loc().selector.unwrap();
        let message = format!("Prefer `JSON.parse` over `JSON.{method}`.");
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.replace(selector, "parse");
        });
    }
}
