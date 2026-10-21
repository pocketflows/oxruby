//! Port of `lib/rubocop/cop/lint/super_argument_mismatch.rb`.
//!
//! The cop resolves the parent method through the project index
//! (`ProjectIndexHelp`, the `rubydex` gem). oxcop has no project index, so,
//! like RuboCop without one, it never registers an offense.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SuperArgumentMismatch;

impl Cop for SuperArgumentMismatch {
    fn name(&self) -> &'static str {
        "Lint/SuperArgumentMismatch"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Super]
    }

    fn on_node<'a>(&mut self, _cx: &mut Investigation<'a, '_>, _node: Node<'a>) {
        // `return unless project_index`
    }
}
