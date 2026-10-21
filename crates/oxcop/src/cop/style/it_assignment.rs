//! Port of `lib/rubocop/cop/style/it_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "`it` is the default block parameter; consider another name.";

#[derive(Default)]
pub struct ItAssignment;

impl Cop for ItAssignment {
    fn name(&self) -> &'static str {
        "Style/ItAssignment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Lvasgn,
            NodeType::Arg,
            NodeType::Optarg,
            NodeType::Restarg,
            NodeType::Blockarg,
            NodeType::Kwarg,
            NodeType::Kwoptarg,
            NodeType::Kwrestarg,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.name() != Some("it") {
            return;
        }
        if let Some(name) = node.loc().name {
            cx.add_offense(name, MSG);
        }
    }
}
