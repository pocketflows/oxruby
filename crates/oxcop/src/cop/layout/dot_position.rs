//! Port of `lib/rubocop/cop/layout/dot_position.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct DotPosition;

impl Cop for DotPosition {
    fn name(&self) -> &'static str {
        "Layout/DotPosition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/RedundantSelf"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_dot() || node.is_safe_navigation()) {
            return;
        }
        let style = style(cx);
        let ps = cx.processed_source;
        if proper_dot_position(ps, style, node) {
            return;
        }
        let dot = node.loc().dot.unwrap();
        let message = message(ps, style, dot);
        cx.add_offense_with_correction(dot, &message, |corrector| {
            autocorrect(ps, style, corrector, dot, node);
        });
    }
}

fn autocorrect(
    ps: &ProcessedSource<'_>,
    style: &str,
    corrector: &mut Corrector,
    dot: Range,
    node: Node<'_>,
) {
    let dot_range = if ps.source_line(ps.line_of(dot.begin_pos)).trim() == "." {
        range_by_whole_lines(ps, dot, true)
    } else {
        dot
    };
    corrector.remove(dot_range);
    match style {
        "leading" => corrector.insert_before(selector_range(node), ps.slice(dot)),
        "trailing" => corrector.insert_after(node.receiver().unwrap().source_range(), ps.slice(dot)),
        _ => {}
    }
}

fn message(ps: &ProcessedSource<'_>, style: &str, dot: Range) -> String {
    let tail = match style {
        "leading" => "next line, together with the method name.",
        "trailing" => "previous line, together with the method call receiver.",
        _ => "",
    };
    format!("Place the {} on the {tail}", ps.slice(dot))
}

fn proper_dot_position(ps: &ProcessedSource<'_>, style: &str, node: Node<'_>) -> bool {
    let selector_range = selector_range(node);
    let receiver = node.receiver().unwrap();
    let receiver_end = receiver.source_range().end_pos;
    if ps.line_of(selector_range.begin_pos) == ps.line_of(receiver_end) {
        return true;
    }
    let selector_line = ps.line_of(selector_range.begin_pos);
    let receiver_line = receiver_end_line(ps, receiver);
    let dot_line = ps.line_of(node.loc().dot.unwrap().begin_pos);

    if line_between(selector_line, receiver_line.max(dot_line)) {
        return true;
    }
    correct_dot_position_style(style, dot_line, selector_line)
}

fn line_between(first_line: usize, second_line: usize) -> bool {
    first_line as isize - second_line as isize > 1
}

fn correct_dot_position_style(style: &str, dot_line: usize, selector_line: usize) -> bool {
    match style {
        "leading" => dot_line == selector_line,
        "trailing" => dot_line != selector_line,
        _ => false,
    }
}

fn receiver_end_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    match last_heredoc_line(ps, node) {
        Some(line) => line,
        None => ps.line_of(node.source_range().end_pos),
    }
}

fn last_heredoc_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> Option<usize> {
    if node.is_call() {
        node.arguments()
            .into_iter()
            .filter(|arg| heredoc(*arg))
            .map(|arg| ps.line_of(arg.loc().heredoc_end.unwrap().begin_pos))
            .max()
    } else if heredoc(node) {
        Some(ps.line_of(node.loc().heredoc_end.unwrap().begin_pos))
    } else {
        None
    }
}

fn heredoc(node: Node<'_>) -> bool {
    node.is_any_str() && node.is_heredoc()
}

fn selector_range(node: Node<'_>) -> Range {
    if !node.is_call() {
        return node.source_range();
    }
    node.loc().selector.or(node.loc().begin).unwrap()
}
