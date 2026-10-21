//! Port of `lib/rubocop/cop/style/hash_slice.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::hash_subset::{self, semantically_slice_method};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct HashSlice;

impl Cop for HashSlice {
    fn name(&self) -> &'static str {
        "Style/HashSlice"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.5)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        hash_subset::RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        hash_subset::on_send(cx, node, "slice", semantically_slice_method);
    }
}
