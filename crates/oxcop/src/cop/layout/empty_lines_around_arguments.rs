//! Port of `lib/rubocop/cop/layout/empty_lines_around_arguments.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{SurroundingSpace, Side, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Empty line detected around arguments.";

#[derive(Default)]
pub struct EmptyLinesAroundArguments;

impl Cop for EmptyLinesAroundArguments {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundArguments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ps = cx.processed_source;
        if node.is_single_line()
            || node.arguments().is_empty()
            || receiver_and_method_call_on_different_lines(ps, node)
        {
            return;
        }
        let mut ranges = Vec::new();
        for arg in node.arguments() {
            ranges.extend(empty_range_for_starting_point(ps, arg.source_range().begin()));
        }
        if let Some(end) = node.loc().end {
            ranges.extend(empty_range_for_starting_point(ps, end.begin()));
        }
        for range in ranges {
            cx.add_offense_with_correction(range, MSG, |corrector| corrector.remove(range));
        }
    }
}

fn receiver_and_method_call_on_different_lines(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    node.receiver().is_some_and(|receiver| {
        Some(receiver.last_line()) != node.loc().selector.map(|s| ps.line_of(s.begin_pos))
    })
}

fn empty_range_for_starting_point(ps: &ProcessedSource<'_>, start: Range) -> Option<Range> {
    let range = range_with_surrounding_space(
        ps.buffer_source(),
        start,
        SurroundingSpace {
            side: Side::Left,
            whitespace: true,
            ..SurroundingSpace::default()
        },
    );
    let first_line = ps.line_of(range.begin_pos);
    let last_line = ps.line_of(range.end_pos);
    if last_line - first_line <= 1 {
        return None;
    }
    let line_range = ps.line_range(last_line - 1);
    Some(Range::new(line_range.begin_pos, line_range.end_pos + 1))
}
