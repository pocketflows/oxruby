//! Port of `lib/rubocop/cop/layout/empty_line_between_defs.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct EmptyLineBetweenDefs;

impl Cop for EmptyLineBetweenDefs {
    fn name(&self) -> &'static str {
        "Layout/EmptyLineBetweenDefs"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Layout/EmptyLines"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Begin]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        for pair in children.windows(2) {
            if pair.iter().all(|n| candidate(cx, *n)) {
                check_defs(cx, pair[0], pair[1]);
            }
        }
    }
}

fn check_defs<'a>(cx: &mut Investigation<'a, '_>, prev: Node<'a>, node: Node<'a>) {
    let ps = cx.processed_source;
    let count = blank_lines_count_between(ps, prev, node);
    let (minimum, maximum) = number_of_empty_lines(cx);
    if (minimum..=maximum).contains(&count) {
        return;
    }
    if multiple_blank_lines_groups(ps, prev, node) {
        return;
    }
    if prev.is_single_line() && node.is_single_line() && cx.cop_config_bool("AllowAdjacentOneLineDefs", false) {
        return;
    }
    let location = def_location(node);
    let message = message(node, minimum, maximum, count);
    cx.add_offense_with_correction(location, &message, |corrector| {
        autocorrect(ps, corrector, prev, node, count, minimum, maximum);
    });
}

fn autocorrect(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    prev_def: Node<'_>,
    node: Node<'_>,
    count: i64,
    minimum: i64,
    maximum: i64,
) {
    let end_pos = end_loc(prev_def).end_pos;
    let source = ps.buffer_source();
    let mut newline_pos = source[end_pos..]
        .find('\n')
        .map_or(source.len(), |i| end_pos + i);
    let begin_pos = node.source_range().begin_pos;
    if newline_pos > begin_pos {
        newline_pos = begin_pos - 1;
    }
    if count > maximum {
        let difference = (count - maximum) as usize;
        corrector.remove(range_between(newline_pos, newline_pos + difference));
    } else {
        let difference = (minimum - count) as usize;
        let where_to_insert = range_between(newline_pos, newline_pos + 1);
        corrector.insert_after(where_to_insert, "\n".repeat(difference));
    }
}

fn def_location(node: Node<'_>) -> Range {
    if node.is_any_block() {
        node.source_range().join(node.child(0).unwrap().source_range())
    } else if node.is_send() {
        node.source_range()
    } else {
        node.loc().keyword.unwrap().join(node.loc().name.unwrap())
    }
}

fn candidate(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    method_candidate(cx, node)
        || class_candidate(cx, node)
        || module_candidate(cx, node)
        || macro_candidate(cx, node)
}

fn macro_candidate(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let macro_candidate = if node.is_any_block() {
        node.send_node()
    } else if node.is_send() {
        Some(node)
    } else {
        None
    };
    let Some(macro_candidate) = macro_candidate else {
        return false;
    };
    macro_candidate.is_macro()
        && macro_candidate
            .method_name()
            .is_some_and(|name| cx.cop_config_list("DefLikeMacros").contains(&name))
}

fn method_candidate(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("EmptyLineBetweenMethodDefs", false) && node.is_any_def()
}

fn class_candidate(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("EmptyLineBetweenClassDefs", false) && node.is(NodeType::Class)
}

fn module_candidate(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("EmptyLineBetweenModuleDefs", false) && node.is(NodeType::Module)
}

fn message(node: Node<'_>, minimum: i64, maximum: i64, count: i64) -> String {
    format!(
        "Expected {} between {} definitions; found {count}.",
        expected_lines(minimum, maximum),
        node_type(node)
    )
}

fn expected_lines(minimum: i64, maximum: i64) -> String {
    if minimum != maximum {
        format!("{minimum}..{maximum} empty lines")
    } else {
        let lines = if maximum == 1 { "line" } else { "lines" };
        format!("{maximum} empty {lines}")
    }
}

fn blank(line: &str) -> bool {
    line.chars().all(char::is_whitespace)
}

fn multiple_blank_lines_groups(ps: &ProcessedSource<'_>, first: Node<'_>, second: Node<'_>) -> bool {
    let lines = lines_between_defs(ps, first, second);
    let blank_start = (0..lines.len()).filter(|&i| blank(lines[i])).max();
    let non_blank_end = (0..lines.len()).filter(|&i| !blank(lines[i])).min();
    match (blank_start, non_blank_end) {
        (Some(blank_start), Some(non_blank_end)) => blank_start > non_blank_end,
        _ => false,
    }
}

fn blank_lines_count_between(ps: &ProcessedSource<'_>, first: Node<'_>, second: Node<'_>) -> i64 {
    lines_between_defs(ps, first, second)
        .iter()
        .filter(|l| blank(l))
        .count() as i64
}

/// `Array(cop_config['NumberOfEmptyLines'])`: its first and last values.
fn number_of_empty_lines(cx: &Investigation<'_, '_>) -> (i64, i64) {
    match cx.cop_config_value("NumberOfEmptyLines") {
        Some(Value::Sequence(items)) => {
            let first = items.first().and_then(Value::as_i64).unwrap_or(0);
            let last = items.last().and_then(Value::as_i64).unwrap_or(0);
            (first, last)
        }
        Some(value) => {
            let n = value.as_i64().unwrap_or(0);
            (n, n)
        }
        None => (0, 0),
    }
}

fn lines_between_defs<'s>(ps: &ProcessedSource<'s>, first: Node<'_>, second: Node<'_>) -> Vec<&'s str> {
    let begin_line_num = def_end(ps, first);
    let end_line_num = def_start(ps, second) as isize - 2;
    if end_line_num < 0 {
        return Vec::new();
    }
    let lines = ps.lines();
    let end = (end_line_num as usize + 1).min(lines.len());
    if begin_line_num >= end {
        return Vec::new();
    }
    lines[begin_line_num..end].to_vec()
}

fn def_start(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let node = if node.is_any_block() {
        node.send_node().unwrap()
    } else {
        node
    };
    if node.is_send() {
        node.first_line()
    } else {
        ps.line_of(node.loc().keyword.unwrap().begin_pos)
    }
}

fn def_end(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    ps.line_of(end_loc(node).begin_pos)
}

fn end_loc(node: Node<'_>) -> Range {
    let end_location = node.source_range().end();
    trailing_heredoc_end(node, end_location).unwrap_or(end_location)
}

fn trailing_heredoc_end(node: Node<'_>, end_location: Range) -> Option<Range> {
    let heredoc_end = node
        .descendants()
        .filter(|d| d.is_any_str() && d.is_heredoc())
        .filter_map(|h| h.loc().heredoc_end)
        .max_by_key(|r| r.end_pos)?;
    (heredoc_end.end_pos > end_location.end_pos).then_some(heredoc_end)
}

fn node_type(node: Node<'_>) -> &'static str {
    match node.ty() {
        NodeType::Def | NodeType::Defs => "method",
        NodeType::Numblock | NodeType::Itblock => "block",
        other => other.name(),
    }
}
