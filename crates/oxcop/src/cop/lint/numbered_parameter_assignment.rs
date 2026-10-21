//! Port of `lib/rubocop/cop/lint/numbered_parameter_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct NumberedParameterAssignment;

impl Cop for NumberedParameterAssignment {
    fn name(&self) -> &'static str {
        "Lint/NumberedParameterAssignment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Lvasgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(digits) = node.name().and_then(|name| name.strip_prefix('_')) else {
            return;
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return;
        }
        // `Regexp.last_match(1).to_i`
        let number = digits.trim_start_matches('0');
        let number = if number.is_empty() { "0" } else { number };
        let message = if number.len() == 1 && number != "0" {
            format!("`_{number}` is reserved for numbered parameter; consider another name.")
        } else {
            format!("`_{number}` is similar to numbered parameter; consider another name.")
        };
        cx.add_offense(node.source_range(), &message);
    }
}
