//! Port of `lib/rubocop/cop/lint/duplicate_elsif_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Duplicate `elsif` condition detected.";

#[derive(Default)]
pub struct DuplicateElsifCondition;

impl Cop for DuplicateElsifCondition {
    fn name(&self) -> &'static str {
        "Lint/DuplicateElsifCondition"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let mut previous: Vec<Node<'a>> = Vec::new();
        let mut node = node;
        while node.is_if() || node.is_elsif() {
            let Some(condition) = node.condition() else {
                break;
            };
            if previous.iter().any(|p| p.same_as(condition)) {
                cx.add_offense(condition.source_range(), MSG);
            }
            previous.push(condition);
            match node.else_branch() {
                Some(else_branch) if else_branch.is(NodeType::If) => node = else_branch,
                _ => break,
            }
        }
    }
}
