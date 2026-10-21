//! Port of `lib/rubocop/cop/lint/hash_compare_by_identity.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `Hash#compare_by_identity` instead of using `object_id` for keys.";

static ID_AS_HASH_KEY: NodePattern = NodePattern::new(
    "(call _ {:key? :has_key? :fetch :[] :[]=} (send _ :object_id) ...)",
);

#[derive(Default)]
pub struct HashCompareByIdentity;

impl Cop for HashCompareByIdentity {
    fn name(&self) -> &'static str {
        "Lint/HashCompareByIdentity"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["key?", "has_key?", "fetch", "[]", "[]="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if ID_AS_HASH_KEY.is_match(node) {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}
