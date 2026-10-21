//! Port of `lib/rubocop/cop/correctors/alignment_corrector.rb`.

use crate::ast::{Node, NodeType};
use crate::config::Config;
use crate::cop::Corrector;
use crate::cop::mixin::alignment::within;
use crate::cop::mixin::range_help::range_between;
use crate::processed_source::{ProcessedSource, Range};

/// What to align: a node, or a bare range (`node.respond_to?(:loc)`).
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    Node(Node<'a>),
    Range(Range),
}

impl Target<'_> {
    fn range(&self) -> Range {
        match self {
            Target::Node(node) => node.source_range(),
            Target::Range(range) => *range,
        }
    }
}

/// `AlignmentCorrector.correct(corrector, processed_source, node,
/// column_delta, tab_indentation:)`.
pub fn correct(
    corrector: &mut Corrector,
    ps: &ProcessedSource<'_>,
    config: &Config,
    node: Option<Target<'_>>,
    column_delta: isize,
    tab_indentation: bool,
) {
    let Some(node) = node else {
        return;
    };
    let expr = node.range();
    if block_comment_within(ps, expr) {
        return;
    }
    let taboo_ranges = inside_string_ranges(node);
    if using_tabs(config) {
        if !tab_indentation {
            return;
        }
        correct_tab_indentation(corrector, ps, config, expr, column_delta, &taboo_ranges);
    } else {
        for line_begin_pos in each_line(ps, expr) {
            autocorrect_line(corrector, ps, line_begin_pos, column_delta, &taboo_ranges);
        }
    }
}

/// `AlignmentCorrector.align_end(corrector, processed_source, node, align_to)`.
pub fn align_end(
    corrector: &mut Corrector,
    ps: &ProcessedSource<'_>,
    config: &Config,
    node: Node<'_>,
    align_to: Option<Target<'_>>,
) {
    let whitespace = whitespace_range(ps, node);
    let column = alignment_column(ps, align_to);
    let indentation = indentation_string(config, column);
    if ps.slice(whitespace).trim().is_empty() {
        corrector.replace(whitespace, indentation);
    } else {
        corrector.insert_after(whitespace, format!("\n{indentation}"));
    }
}

fn autocorrect_line(
    corrector: &mut Corrector,
    ps: &ProcessedSource<'_>,
    line_begin_pos: usize,
    column_delta: isize,
    taboo_ranges: &[Range],
) {
    let src = ps.buffer_source();
    let range = calculate_range(src, line_begin_pos, column_delta);
    if taboo_ranges.iter().any(|t| within(range, *t)) {
        return;
    }
    if column_delta > 0 && src.get(range.begin_pos..range.begin_pos + 1) != Some("\n") {
        corrector.insert_before(range, " ".repeat(column_delta as usize));
    } else {
        let source = &src[range.begin_pos..range.end_pos];
        if !source.is_empty() && source.bytes().all(|b| b == b' ' || b == b'\t') {
            corrector.remove(range);
        }
    }
}

fn correct_tab_indentation(
    corrector: &mut Corrector,
    ps: &ProcessedSource<'_>,
    config: &Config,
    expr: Range,
    column_delta: isize,
    taboo_ranges: &[Range],
) {
    for line_begin_pos in each_line(ps, expr) {
        let line_range = ps.line_range(ps.line_of(line_begin_pos));
        correct_tab_line(
            corrector,
            ps,
            config,
            line_range,
            column_delta,
            taboo_ranges,
        );
    }
}

fn correct_tab_line(
    corrector: &mut Corrector,
    ps: &ProcessedSource<'_>,
    config: &Config,
    line_range: Range,
    column_delta: isize,
    taboo_ranges: &[Range],
) {
    let source = ps.slice(line_range);
    let leading_len = source.len() - source.trim_start_matches([' ', '\t']).len();
    if leading_len == source.len() {
        // blank line
        return;
    }
    let leading = &source[..leading_len];
    let leading_range = range_between(line_range.begin_pos, line_range.begin_pos + leading_len);
    if taboo_ranges.iter().any(|t| within(leading_range, *t)) {
        return;
    }
    let target = target_tab_indentation(config, leading, column_delta);
    if leading != target {
        corrector.replace(leading_range, target);
    }
}

fn target_tab_indentation(config: &Config, leading: &str, column_delta: isize) -> String {
    let width = configured_indentation_width(config);
    if width <= 0 {
        return leading.to_owned();
    }
    let visual_width: i64 = leading
        .chars()
        .map(|c| if c == '\t' { width } else { 1 })
        .sum();
    "\t".repeat(((visual_width + column_delta as i64).max(0) / width) as usize)
}

fn configured_indentation_width(config: &Config) -> i64 {
    config
        .for_cop("Layout/IndentationWidth")
        .get("Width")
        .and_then(serde_yaml_ng::Value::as_i64)
        .unwrap_or(2)
}

fn inside_string_ranges(node: Target<'_>) -> Vec<Range> {
    let Target::Node(node) = node else {
        return Vec::new();
    };
    node.each_node()
        .filter(|n| matches!(n.ty(), NodeType::Str | NodeType::Dstr | NodeType::Xstr))
        .filter_map(inside_string_range)
        .collect()
}

fn inside_string_range(node: Node<'_>) -> Option<Range> {
    let loc = node.loc();
    if node.is_heredoc() {
        Some(loc.heredoc_body?.join(loc.heredoc_end?))
    } else if let (Some(begin), Some(end)) = (loc.begin, loc.end) {
        Some(begin.end().join(end.begin()))
    } else {
        None
    }
}

fn block_comment_within(ps: &ProcessedSource<'_>, expr: Range) -> bool {
    ps.comments()
        .iter()
        .any(|c| c.is_document() && within(c.range, expr))
}

fn calculate_range(src: &str, line_begin_pos: usize, column_delta: isize) -> Range {
    if column_delta > 0 {
        return range_between(line_begin_pos, line_begin_pos);
    }
    let delta = column_delta.unsigned_abs();
    if src[line_begin_pos..].starts_with(' ') {
        range_between(line_begin_pos, (line_begin_pos + delta).min(src.len()))
    } else {
        range_between(line_begin_pos.saturating_sub(delta), line_begin_pos)
    }
}

/// `each_line(expr)`: the start of `expr`, then the start of each further
/// line of it.
fn each_line(ps: &ProcessedSource<'_>, expr: Range) -> Vec<usize> {
    let mut line_begin_pos = expr.begin_pos;
    let mut positions = Vec::new();
    for line in ps.slice(expr).split_inclusive('\n') {
        positions.push(line_begin_pos);
        line_begin_pos += line.len();
    }
    positions
}

fn whitespace_range(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let begin_pos = node.loc().end.unwrap().begin_pos;
    range_between(ps.line_start(ps.line_of(begin_pos)), begin_pos)
}

fn alignment_column(ps: &ProcessedSource<'_>, align_to: Option<Target<'_>>) -> usize {
    match align_to {
        None => 0,
        Some(target) => ps.column_of(target.range().begin_pos),
    }
}

fn indentation_string(config: &Config, column: usize) -> String {
    if using_tabs(config) {
        "\t".repeat(column)
    } else {
        " ".repeat(column)
    }
}

fn using_tabs(config: &Config) -> bool {
    config
        .for_cop("Layout/IndentationStyle")
        .get("EnforcedStyle")
        .and_then(serde_yaml_ng::Value::as_str)
        == Some("tabs")
}
