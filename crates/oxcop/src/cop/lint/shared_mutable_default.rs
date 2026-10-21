//! Port of `lib/rubocop/cop/lint/shared_mutable_default.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not create a Hash with a mutable default value as the default value can accidentally be changed.";

static HASH_INITIALIZED_WITH_MUTABLE_SHARED_OBJECT: NodePattern = NodePattern::new(
    "{
       (send (const {nil? cbase} :Hash) :new [
         {array hash (send (const {nil? cbase} {:Array :Hash}) :new)}
         !#capacity_keyword_argument?
       ])
       (send (const {nil? cbase} :Hash) :new
         {array hash (send (const {nil? cbase} {:Array :Hash}) :new)}
         #capacity_keyword_argument?)
     }",
);

static CAPACITY_KEYWORD_ARGUMENT: NodePattern =
    NodePattern::new("(hash (pair (sym :capacity) _))");

#[derive(Default)]
pub struct SharedMutableDefault;

impl Cop for SharedMutableDefault {
    fn name(&self) -> &'static str {
        "Lint/SharedMutableDefault"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let call = |name: &str, val: &Val<'_>, _args: &[Val<'_>]| {
            name == "capacity_keyword_argument?"
                && val
                    .node()
                    .is_some_and(|n| CAPACITY_KEYWORD_ARGUMENT.is_match(n))
        };
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        if HASH_INITIALIZED_WITH_MUTABLE_SHARED_OBJECT
            .match_with(node, &ctx)
            .is_none()
        {
            return;
        }

        cx.add_offense(node.source_range(), MSG);
    }
}
