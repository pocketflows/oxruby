//! Port of `lib/rubocop/cop/layout/space_before_brackets.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove the space before the opening brackets.";

#[derive(Default)]
pub struct SpaceBeforeBrackets;

impl Cop for SpaceBeforeBrackets {
    fn name(&self) -> &'static str {
        "Layout/SpaceBeforeBrackets"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]", "[]="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.loc().dot.is_some() {
            return;
        }
        let (Some(receiver), Some(selector)) = (node.receiver(), node.loc().selector) else {
            return;
        };
        let receiver_end_pos = receiver.source_range().end_pos;
        let selector_begin_pos = selector.begin_pos;
        if receiver_end_pos >= selector_begin_pos {
            return;
        }
        let range = range_between(receiver_end_pos, selector_begin_pos);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.remove(range);
        });
    }
}
