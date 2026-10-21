//! Port of `lib/rubocop/cop/style/min_max.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static MIN_MAX_CANDIDATE: NodePattern = NodePattern::new(
    "({array return} (send [$_receiver !nil?] :min) (send [$_receiver !nil?] :max))",
);

#[derive(Default)]
pub struct MinMax;

impl Cop for MinMax {
    fn name(&self) -> &'static str {
        "Style/MinMax"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array, NodeType::Return]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = MIN_MAX_CANDIDATE.match_node(node).and_then(|c| c[0].node()) else {
            return;
        };
        let offender = offending_range(node);
        let message = format!(
            "Use `{}.minmax` instead of `{}`.",
            receiver.source(),
            node.range_source(offender)
        );
        cx.add_offense_with_correction(offender, &message, |corrector| {
            let receiver = node.child(0).unwrap().receiver().unwrap();
            corrector.replace(offender, format!("{}.minmax", receiver.source()));
        });
    }
}

fn offending_range(node: Node<'_>) -> Range {
    if node.is(NodeType::Return) {
        let first = node.child_nodes().next().unwrap().source_range();
        let last = node.child_nodes().last().unwrap().source_range();
        first.join(last)
    } else {
        node.source_range()
    }
}
