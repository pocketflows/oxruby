//! Port of `lib/rubocop/cop/lint/or_assignment_to_constant.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid using or-assignment with constants.";

#[derive(Default)]
pub struct OrAssignmentToConstant;

impl Cop for OrAssignmentToConstant {
    fn name(&self) -> &'static str {
        "Lint/OrAssignmentToConstant"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::OrAsgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.lhs().is_some_and(|lhs| lhs.is(NodeType::Casgn)) {
            return;
        }
        let operator = node.loc().operator.unwrap();
        cx.add_offense_with_correction(operator, MSG, |corrector| {
            if node.ancestors().any(|a| a.is_any_def()) {
                return;
            }
            corrector.replace(operator, "=");
        });
    }
}
