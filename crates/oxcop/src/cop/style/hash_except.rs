//! Port of `lib/rubocop/cop/style/hash_except.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::hash_subset::{self, semantically_except_method};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct HashExcept;

impl Cop for HashExcept {
    fn name(&self) -> &'static str {
        "Style/HashExcept"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        hash_subset::RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        hash_subset::on_send(cx, node, "except", semantically_except_method);
    }
}
