//! Port of `lib/rubocop/cop/lint/duplicate_rescue_exception.rb`.
//!
//! `rescue_modifier?(node)` (`RescueNode`) is always false here: it is
//! called with the `rescue` node, which is not a `resbody`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Duplicate `rescue` exception detected.";

#[derive(Default)]
pub struct DuplicateRescueException;

impl Cop for DuplicateRescueException {
    fn name(&self) -> &'static str {
        "Lint/DuplicateRescueException"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Rescue]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let mut previous: Vec<Node<'a>> = Vec::new();
        for resbody in node.resbody_branches() {
            for exception in resbody.exceptions() {
                if previous.iter().any(|p| p.same_as(exception)) {
                    cx.add_offense(exception.source_range(), MSG);
                } else {
                    previous.push(exception);
                }
            }
        }
    }
}
