//! Port of `lib/rubocop/cop/lint/duplicate_hash_key.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::duplication::{consecutive_duplicates, has_duplicates};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Duplicated key in hash literal.";

#[derive(Default)]
pub struct DuplicateHashKey;

impl Cop for DuplicateHashKey {
    fn name(&self) -> &'static str {
        "Lint/DuplicateHashKey"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let keys: Vec<Node<'a>> = node
            .hash_keys()
            .into_iter()
            .filter(|key| key.is_recursive_basic_literal() || key.is(NodeType::Const))
            .collect();
        let same = |a: Node<'_>, b: Node<'_>| a.same_as(b);
        if !has_duplicates(&keys, same) {
            return;
        }
        for key in consecutive_duplicates(&keys, same) {
            cx.add_offense(key.source_range(), MSG);
        }
    }
}
