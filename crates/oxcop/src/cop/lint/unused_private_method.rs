//! Port of `lib/rubocop/cop/lint/unused_private_method.rb`.
//!
//! The cop finds private methods through the project index
//! (`ProjectIndexHelp`, the `rubydex` gem). oxcop has no project index, so,
//! like RuboCop without one, it never registers an offense.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct UnusedPrivateMethod;

impl Cop for UnusedPrivateMethod {
    fn name(&self) -> &'static str {
        "Lint/UnusedPrivateMethod"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def]
    }

    fn on_node<'a>(&mut self, _cx: &mut Investigation<'a, '_>, _node: Node<'a>) {
        // `return unless project_index`
    }
}
