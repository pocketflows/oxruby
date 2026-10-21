//! Port of `lib/rubocop/cop/style/hash_transform_keys.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::hash_transform_method::{HashTransform, RESTRICT_ON_SEND};
use crate::cop::{Cop, Investigation};

static ON_BAD_EACH_WITH_OBJECT: NodePattern = NodePattern::new(
    "(block
       (call #hash_receiver? :each_with_object (hash))
       (args
         (mlhs
           (arg $_)
           (arg _val))
         (arg _memo))
       (call (lvar _memo) :[]= $!`_memo $(lvar _val)))",
);
static ON_BAD_HASH_BRACKETS_MAP: NodePattern = NodePattern::new(
    "(send
       (const _ :Hash)
       :[]
       (block
         (call #hash_receiver? {:map :collect})
         (args
           (arg $_)
           (arg _val))
         (array $_ $(lvar _val))))",
);
static ON_BAD_MAP_TO_H: NodePattern = NodePattern::new(
    "(call
       (block
         (call #hash_receiver? {:map :collect})
         (args
           (arg $_)
           (arg _val))
         (array $_ $(lvar _val)))
       :to_h)",
);
static ON_BAD_TO_H: NodePattern = NodePattern::new(
    "(block
       (call #hash_receiver? :to_h)
       (args
         (arg $_)
         (arg _val))
       (array $_ $(lvar _val)))",
);

static TRANSFORM: HashTransform = HashTransform {
    new_method_name: "transform_keys",
    on_bad_each_with_object: &ON_BAD_EACH_WITH_OBJECT,
    on_bad_hash_brackets_map: &ON_BAD_HASH_BRACKETS_MAP,
    on_bad_map_to_h: &ON_BAD_MAP_TO_H,
    on_bad_to_h: &ON_BAD_TO_H,
    captures: [0, 1, 2],
};

#[derive(Default)]
pub struct HashTransformKeys;

impl Cop for HashTransformKeys {
    fn name(&self) -> &'static str {
        "Style/HashTransformKeys"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.5)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Block => TRANSFORM.on_block(cx, node),
            NodeType::Send => TRANSFORM.on_send(cx, node),
            _ => TRANSFORM.on_csend(cx, node),
        }
    }
}
