//! Port of `lib/rubocop/cop/style/time_now.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer `Time.now` over `Time.new` to retrieve the current time.";

static TIME_NEW: NodePattern = NodePattern::new("(call (const {nil? cbase} :Time) :new)");

#[derive(Default)]
pub struct TimeNow;

impl Cop for TimeNow {
    fn name(&self) -> &'static str {
        "Style/TimeNow"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !TIME_NEW.is_match(node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let selector = node.loc().selector.unwrap();
            corrector.replace(selector.join(node.source_range().end()), "now");
        });
    }
}
