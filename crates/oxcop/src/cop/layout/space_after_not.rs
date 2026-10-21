//! Port of `lib/rubocop/cop/layout/space_after_not.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not leave space between `!` and its argument.";

#[derive(Default)]
pub struct SpaceAfterNot;

impl Cop for SpaceAfterNot {
    fn name(&self) -> &'static str {
        "Layout/SpaceAfterNot"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_prefix_bang() && whitespace_after_operator(node)) {
            return;
        }
        let range = range_between(
            node.loc().selector.unwrap().end_pos,
            node.receiver().unwrap().source_range().begin_pos,
        );
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.remove(range);
        });
    }
}

fn whitespace_after_operator(node: Node<'_>) -> bool {
    node.receiver().unwrap().source_range().begin_pos - node.source_range().begin_pos > 1
}
