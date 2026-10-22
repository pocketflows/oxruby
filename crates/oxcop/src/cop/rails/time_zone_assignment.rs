//! Port of rubocop-rails' `lib/rubocop/cop/rails/time_zone_assignment.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `Time.use_zone` with block instead of `Time.zone=`.";

static TIME_ZONE_ASSIGNMENT: NodePattern =
    NodePattern::new("(send (const {nil? cbase} :Time) :zone= ...)");

#[derive(Default)]
pub struct TimeZoneAssignment;

impl Cop for TimeZoneAssignment {
    fn name(&self) -> &'static str {
        "Rails/TimeZoneAssignment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["zone="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !TIME_ZONE_ASSIGNMENT.is_match(node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}
