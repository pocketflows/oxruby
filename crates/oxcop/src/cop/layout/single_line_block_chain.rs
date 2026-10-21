//! Port of `lib/rubocop/cop/layout/single_line_block_chain.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Put method call on a separate line if chained to a single line block.";

#[derive(Default)]
pub struct SingleLineBlockChain;

impl Cop for SingleLineBlockChain {
    fn name(&self) -> &'static str {
        "Layout/SingleLineBlockChain"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/MapToHash"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(range) = offending_range(node) {
            cx.add_offense_with_correction(range, MSG, |corrector| {
                corrector.insert_before(range, "\n");
            });
        }
    }
}

fn offending_range(node: Node<'_>) -> Option<Range> {
    let receiver = node.receiver().filter(|r| r.is_any_block())?;
    let receiver_location = receiver.loc();
    let closing_block_delimiter_line_num = receiver.line_of(receiver_location.end?);
    if receiver.line_of(receiver_location.begin?) < closing_block_delimiter_line_num {
        return None;
    }
    let dot_range = node.loc().dot?;
    if !call_method_after_block(node, dot_range, closing_block_delimiter_line_num) {
        return None;
    }
    Some(range_between(
        dot_range.begin_pos,
        selector_range(node)?.end_pos,
    ))
}

fn call_method_after_block(
    node: Node<'_>,
    dot_range: Range,
    closing_block_delimiter_line_num: usize,
) -> bool {
    if node.line_of(dot_range) > closing_block_delimiter_line_num {
        return false;
    }
    selector_range(node)
        .is_some_and(|selector| node.column_of(dot_range.begin_pos) < node.column_of(selector.begin_pos))
}

fn selector_range(node: Node<'_>) -> Option<Range> {
    node.loc().selector.or(node.loc().begin)
}
