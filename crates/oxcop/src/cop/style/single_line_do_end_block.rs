//! Port of `lib/rubocop/cop/style/single_line_do_end_block.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::check_single_line_suitability::suitable_as_single_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Prefer multiline `do`...`end` block.";

#[derive(Default)]
pub struct SingleLineDoEndBlock;

impl Cop for SingleLineDoEndBlock {
    fn name(&self) -> &'static str {
        "Style/SingleLineDoEndBlock"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_multiline() || node.has_braces() {
            return;
        }
        if single_line_blocks_preferred(cx) && suitable_as_single_line(cx, node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.insert_after(do_line(node), "\n");
            let end = node.loc().end.unwrap();
            if let Some(heredoc) = trailing_heredoc(node.body()) {
                corrector.remove(end);
                corrector.insert_after(heredoc.loc().heredoc_end.unwrap(), "\nend");
            } else {
                corrector.insert_before(end, "\n");
            }
        });
    }
}

fn trailing_heredoc(node_body: Option<Node<'_>>) -> Option<Node<'_>> {
    let node_body = node_body?;
    let mut best: Option<Node<'_>> = None;
    for node in node_body.each_node() {
        if !node.is_heredoc() {
            continue;
        }
        let line = node.last_line_of(node.loc().heredoc_end.unwrap());
        // `max_by` keeps the first maximum.
        if best.is_none_or(|b| line > b.last_line_of(b.loc().heredoc_end.unwrap())) {
            best = Some(node);
        }
    }
    best
}

fn do_line(node: Node<'_>) -> Range {
    if node.is_any(&[NodeType::Numblock, NodeType::Itblock])
        || node.arguments().is_empty()
        || node.send_node().is_some_and(|s| s.is_lambda_literal())
    {
        node.loc().begin.unwrap()
    } else {
        node.arguments_node().unwrap().source_range()
    }
}

fn single_line_blocks_preferred(cx: &Investigation<'_, '_>) -> bool {
    let name = "Layout/RedundantLineBreak";
    cx.config.cop_enabled(name)
        && cx
            .config
            .for_cop(name)
            .get("InspectBlocks")
            .is_some_and(|v| !matches!(v, serde_yaml_ng::Value::Bool(false) | serde_yaml_ng::Value::Null))
}
