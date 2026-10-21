//! Port of `lib/rubocop/cop/lint/heredoc_method_call_position.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{range_between, range_by_whole_lines};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Put a method call with a HEREDOC receiver on the same line as the HEREDOC opening.";

#[derive(Default)]
pub struct HeredocMethodCallPosition;

impl Cop for HeredocMethodCallPosition {
    fn name(&self) -> &'static str {
        "Lint/HeredocMethodCallPosition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(heredoc) = heredoc_node_descendent_receiver(node) else {
            return;
        };
        if correctly_positioned(node, heredoc) {
            return;
        }
        cx.add_offense_cx(call_after_heredoc_range(heredoc), MSG, |corrector, cx| {
            autocorrect(cx.processed_source, corrector, node, heredoc);
        });
    }
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>, heredoc: Node<'_>) {
    let Some(call_range) = call_range_to_safely_reposition(ps, node, heredoc) else {
        return;
    };
    let call_source = ps.slice(call_range).trim();
    corrector.remove(call_range);
    corrector.insert_after(heredoc_begin_line_range(ps, node), call_source);
}

fn heredoc_node_descendent_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let mut node = Some(node);
    while let Some(current) = node.filter(|n| send_node(Some(*n))) {
        if let Some(receiver) = current.receiver()
            && heredoc_node(receiver)
        {
            return Some(receiver);
        }
        node = current.receiver();
    }
    None
}

fn send_node(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| n.is_call())
}

fn heredoc_node(node: Node<'_>) -> bool {
    node.is_any(&[NodeType::Str, NodeType::Dstr, NodeType::Xstr]) && node.is_heredoc()
}

fn call_after_heredoc_range(heredoc: Node<'_>) -> Range {
    let pos = heredoc_end_pos(heredoc);
    range_between(pos + 1, pos + 2)
}

fn correctly_positioned(node: Node<'_>, heredoc: Node<'_>) -> bool {
    heredoc_end_pos(heredoc) > call_end_pos(node)
}

fn calls_on_multiple_lines(node: Node<'_>) -> bool {
    let last_line = node.last_line();
    let mut node = Some(node);
    while let Some(current) = node.filter(|n| send_node(Some(*n))) {
        if last_line != current.last_line() {
            return true;
        }
        if !all_on_same_line(&current.arguments()) {
            return true;
        }
        node = current.receiver();
    }
    false
}

fn all_on_same_line(nodes: &[Node<'_>]) -> bool {
    match (nodes.first(), nodes.last()) {
        (Some(first), Some(last)) => first.first_line() == last.last_line(),
        _ => true,
    }
}

fn heredoc_end_pos(heredoc: Node<'_>) -> usize {
    heredoc.loc().heredoc_end.unwrap().end_pos
}

fn call_end_pos(node: Node<'_>) -> usize {
    node.source_range().end_pos
}

fn heredoc_begin_line_range(ps: &ProcessedSource<'_>, heredoc: Node<'_>) -> Range {
    let pos = heredoc.source_range().begin_pos;
    range_by_whole_lines(ps, range_between(pos, pos), false)
}

fn call_line_range(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let pos = node.source_range().end_pos;
    range_by_whole_lines(ps, range_between(pos, pos), false)
}

fn call_range_to_safely_reposition(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
    heredoc: Node<'_>,
) -> Option<Range> {
    if calls_on_multiple_lines(node) {
        return None;
    }
    let heredoc_end_pos = heredoc_end_pos(heredoc);
    let call_end_pos = call_end_pos(node);
    let call_range = range_between(heredoc_end_pos, call_end_pos);
    let call_line_range = call_line_range(ps, node);
    let call_source = ps.slice(call_range).trim();
    let call_line_source = ps.slice(call_line_range).trim();
    if call_source == call_line_source {
        return Some(call_range);
    }
    if trailing_comma(call_source, call_line_source) {
        return Some(range_between(heredoc_end_pos, call_end_pos + 1));
    }
    None
}

fn trailing_comma(call_source: &str, call_line_source: &str) -> bool {
    format!("{call_source},") == call_line_source
}
