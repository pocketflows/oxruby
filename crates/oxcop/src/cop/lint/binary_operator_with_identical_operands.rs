//! Port of `lib/rubocop/cop/lint/binary_operator_with_identical_operands.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct BinaryOperatorWithIdenticalOperands;

fn message(op: &str) -> String {
    format!("Binary operator `{op}` has identical operands.")
}

impl Cop for BinaryOperatorWithIdenticalOperands {
    fn name(&self) -> &'static str {
        "Lint/BinaryOperatorWithIdenticalOperands"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::And, NodeType::Or]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "==", "!=", "===", "<=>", "=~", "&&", "||", ">", ">=", "<", "<=", "|", "^",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_send() {
            if !node.is_binary_operation() {
                return;
            }
            let (Some(receiver), Some(argument)) = (node.receiver(), node.first_argument()) else {
                return;
            };
            if !receiver.same_as(argument) {
                return;
            }
            cx.add_offense(node.source_range(), &message(node.method_name().unwrap()));
        } else {
            let (Some(lhs), Some(rhs)) = (node.lhs(), node.rhs()) else {
                return;
            };
            if !lhs.same_as(rhs) {
                return;
            }
            cx.add_offense(node.source_range(), &message(node.operator().unwrap()));
        }
    }
}
