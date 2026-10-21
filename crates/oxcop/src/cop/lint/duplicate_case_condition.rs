//! Port of `lib/rubocop/cop/lint/duplicate_case_condition.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Duplicate `when` condition detected.";

#[derive(Default)]
pub struct DuplicateCaseCondition;

impl Cop for DuplicateCaseCondition {
    fn name(&self) -> &'static str {
        "Lint/DuplicateCaseCondition"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Case]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, case_node: Node<'a>) {
        let mut previous: Vec<Node<'a>> = Vec::new();
        for when_node in case_node.when_branches() {
            for condition in when_node.conditions() {
                if previous.iter().any(|p| p.same_as(condition)) {
                    cx.add_offense(condition.source_range(), MSG);
                } else {
                    previous.push(condition);
                }
            }
        }
    }
}
