//! Port of `lib/rubocop/cop/layout/block_end_newline.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::begins_its_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct BlockEndNewline;

impl Cop for BlockEndNewline {
    fn name(&self) -> &'static str {
        "Layout/BlockEndNewline"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_single_line() {
            return;
        }
        let ps = cx.processed_source;
        let end = node.loc().end.unwrap();
        if begins_its_line(ps, end) {
            return;
        }
        let offense_range = offense_range(node);
        if ps.slice(offense_range).trim_start().starts_with(';') {
            return;
        }
        let message = format!(
            "Expression at {}, {} should be on its own line.",
            ps.line_of(end.begin_pos),
            ps.column_of(end.begin_pos) + 1
        );
        cx.add_offense_with_correction(end, &message, |corrector| {
            let replacement = format!("\n{}", ps.slice(offense_range).trim_start());
            if let Some(heredoc) = last_heredoc_argument(node.body()) {
                corrector.remove(offense_range);
                corrector.insert_after(heredoc.loc().heredoc_end.unwrap(), replacement);
            } else {
                corrector.replace(offense_range, replacement);
            }
        });
    }
}

fn last_heredoc_argument(node: Option<Node<'_>>) -> Option<Node<'_>> {
    let node = node.filter(|n| n.is_call())?;
    let heredoc = node
        .arguments()
        .into_iter()
        .rev()
        .find(|arg| arg.is(NodeType::Str) && arg.is_heredoc());
    if heredoc.is_some() {
        return heredoc;
    }
    last_heredoc_argument(node.child(0))
}

fn offense_range(node: Node<'_>) -> Range {
    let last = node.child_nodes().last().unwrap();
    last.source_range().end().join(node.loc().end.unwrap())
}
