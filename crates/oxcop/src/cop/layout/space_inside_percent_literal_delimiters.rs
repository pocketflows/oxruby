//! Port of `lib/rubocop/cop/layout/space_inside_percent_literal_delimiters.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::match_range::each_match_range;
use crate::cop::mixin::percent_literal::process;
use crate::cop::mixin::range_help::{contents_range, range_between};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

const MSG: &str = "Do not use spaces inside percent literal delimiters.";

#[derive(Default)]
pub struct SpaceInsidePercentLiteralDelimiters;

impl Cop for SpaceInsidePercentLiteralDelimiters {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsidePercentLiteralDelimiters"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array, NodeType::Xstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let types: &[&str] = if node.is(NodeType::Array) {
            &["%i", "%I", "%w", "%W"]
        } else {
            &["%x"]
        };
        if !process(node, types) {
            return;
        }
        add_offenses_for_blank_spaces(cx, node);
        add_offenses_for_unnecessary_spaces(cx, node);
    }
}

fn remove_offense(cx: &mut Investigation<'_, '_>, range: Range) {
    cx.add_offense_with_correction(range, MSG, |corrector| corrector.remove(range));
}

fn add_offenses_for_blank_spaces(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let range = body_range(node);
    let source = cx.source(range);
    if source.is_empty() || !source.trim().is_empty() {
        return;
    }
    remove_offense(cx, range);
}

fn add_offenses_for_unnecessary_spaces(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !node.is_single_line() {
        return;
    }
    let contents = contents_range(node);
    let source = cx.source(contents);
    for regex in [regex!(r"\A( +)"), regex!(r"(?<!\\)( +)\z")] {
        for match_range in each_match_range(contents, source, regex) {
            remove_offense(cx, match_range);
        }
    }
}

fn body_range(node: Node<'_>) -> Range {
    range_between(
        node.loc().begin.unwrap().end_pos,
        node.loc().end.unwrap().begin_pos,
    )
}
