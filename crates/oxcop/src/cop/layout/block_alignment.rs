//! Port of `lib/rubocop/cop/layout/block_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style_for;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::begins_its_line;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const STYLE_PARAMETER_NAME: &str = "EnforcedStyleAlignWith";

#[derive(Default)]
pub struct BlockAlignment;

impl Cop for BlockAlignment {
    fn name(&self) -> &'static str {
        "Layout/BlockAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let start = start_for_block_node(node);
        check_block_alignment(cx, start, node);
    }
}

/// `{source:, line:, column:}`.
#[derive(Debug, Clone)]
struct SourceLineColumn {
    source: String,
    line: usize,
    column: usize,
}

fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    style_for(cx, STYLE_PARAMETER_NAME)
}

/// `assignment?`: on a call (`MethodDispatchNode`) it is `setter_method?`.
fn assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

/// `block_end_align_target?(node, child)`.
fn block_end_align_target_pattern(node: Node<'_>, child: Node<'_>) -> bool {
    assignment(node)
        || node.is_any_def()
        || node.is_any(&[NodeType::Splat, NodeType::And, NodeType::Or])
        || (node.is(NodeType::Send) && node.is_method("<<"))
        || (node.is(NodeType::Send)
            && node.receiver() == Some(child)
            && !node.is_method("[]"))
}

fn block_end_align_target(node: Node<'_>) -> Node<'_> {
    let lineage: Vec<Node<'_>> = std::iter::once(node).chain(node.ancestors()).collect();
    for pair in lineage.windows(2) {
        let (current, parent) = (pair[0], pair[1]);
        if end_align_target(current, parent) {
            return current;
        }
    }
    *lineage.last().unwrap()
}

fn end_align_target(node: Node<'_>, parent: Node<'_>) -> bool {
    disqualified_parent(parent, node) || !block_end_align_target_pattern(parent, node)
}

fn disqualified_parent(parent: Node<'_>, node: Node<'_>) -> bool {
    parent.first_line() != node.first_line() && !parent.is(NodeType::Masgn)
}

fn check_block_alignment<'a>(cx: &mut Investigation<'a, '_>, start_node: Node<'a>, block_node: Node<'a>) {
    let ps = cx.processed_source;
    let end_loc = block_node.loc().end.unwrap();
    if !begins_its_line(ps, end_loc) {
        return;
    }
    let style = style(cx);
    let start_node = if style == "start_of_line" {
        start_for_line_node(block_node)
    } else {
        start_node
    };
    let start_loc = start_node.source_range();
    if !(ps.column_of(start_loc.begin_pos) != ps.column_of(end_loc.begin_pos) || style == "start_of_block") {
        return;
    }
    let Some(do_source_line_column) = compute_do_source_line_column(ps, style, block_node, end_loc) else {
        return;
    };
    register_offense(cx, block_node, start_loc, end_loc, do_source_line_column);
}

fn register_offense<'a>(
    cx: &mut Investigation<'a, '_>,
    block_node: Node<'a>,
    start_loc: Range,
    end_loc: Range,
    do_source_line_column: SourceLineColumn,
) {
    let ps = cx.processed_source;
    let style = style(cx);
    let error_source_line_column = if style == "start_of_block" {
        do_source_line_column.clone()
    } else {
        loc_to_source_line_column(ps, start_loc)
    };
    let message = format!(
        "{} is not aligned with {}{}.",
        format_source_line_column(&loc_to_source_line_column(ps, end_loc)),
        format_source_line_column(&error_source_line_column),
        alt_start_msg(ps, style, start_loc, &do_source_line_column)
    );
    cx.add_offense_with_correction(end_loc, &message, |corrector| {
        autocorrect(ps, style, corrector, block_node);
    });
}

fn autocorrect(ps: &ProcessedSource<'_>, style: &str, corrector: &mut Corrector, node: Node<'_>) {
    let ancestor_node = if style == "start_of_block" {
        start_for_block_node(node)
    } else {
        start_for_line_node(node)
    };
    let start_col = compute_start_col(ps, style, ancestor_node, node) as isize;
    let loc_end = node.loc().end.unwrap();
    let delta = start_col - ps.column_of(loc_end.begin_pos) as isize;
    if delta > 0 {
        corrector.insert_before(loc_end, " ".repeat(delta as usize));
    } else if delta < 0 {
        let end_pos = loc_end.begin_pos;
        corrector.remove(range_between(end_pos - delta.unsigned_abs(), end_pos));
    }
}

fn start_for_block_node(block_node: Node<'_>) -> Node<'_> {
    find_lhs_node(block_end_align_target(block_node))
}

fn start_for_line_node(block_node: Node<'_>) -> Node<'_> {
    let start_node = start_for_block_node(block_node);
    let ancestors: Vec<Node<'_>> = start_node.ancestors().collect();
    let start_node = ancestors
        .into_iter()
        .rev()
        .find(|node| node.first_line() == start_node.first_line())
        .unwrap_or(start_node);
    find_lhs_node(start_node)
}

fn find_lhs_node(mut node: Node<'_>) -> Node<'_> {
    while node.is_any(&[NodeType::OpAsgn, NodeType::Masgn]) {
        match node.lhs() {
            Some(lhs) => node = lhs,
            None => break,
        }
    }
    node
}

fn first_non_blank(line: &str) -> Option<usize> {
    line.chars().position(|c| !c.is_whitespace())
}

fn source_line<'s>(ps: &ProcessedSource<'s>, range: Range) -> &'s str {
    ps.source_line(ps.line_of(range.begin_pos))
}

fn compute_do_source_line_column(
    ps: &ProcessedSource<'_>,
    style: &str,
    node: Node<'_>,
    end_loc: Range,
) -> Option<SourceLineColumn> {
    let do_loc = node.loc().begin.unwrap();
    let anchor_loc = do_line_anchor_loc(ps, node, do_loc);
    let line = source_line(ps, anchor_loc);
    let indentation_of_do_line = first_non_blank(line)?;
    let byte_start = line.char_indices().nth(indentation_of_do_line).map_or(line.len(), |(i, _)| i);
    let matched = &line[byte_start..];
    let mut permitted_columns = vec![indentation_of_do_line];
    if style == "either"
        && let Some(column) = first_non_blank(source_line(ps, do_loc))
    {
        permitted_columns.push(column);
    }
    if permitted_columns.contains(&ps.column_of(end_loc.begin_pos)) && style != "start_of_line" {
        return None;
    }
    Some(SourceLineColumn {
        source: matched.to_owned(),
        line: ps.line_of(anchor_loc.begin_pos),
        column: indentation_of_do_line,
    })
}

fn loc_to_source_line_column(ps: &ProcessedSource<'_>, loc: Range) -> SourceLineColumn {
    let source = ps.slice(loc);
    let first = source.split_inclusive('\n').next().unwrap_or("");
    SourceLineColumn {
        source: first.strip_suffix('\n').unwrap_or(first).to_owned(),
        line: ps.line_of(loc.begin_pos),
        column: ps.column_of(loc.begin_pos),
    }
}

fn alt_start_msg(ps: &ProcessedSource<'_>, style: &str, start_loc: Range, slc: &SourceLineColumn) -> String {
    if style != "either"
        || (ps.line_of(start_loc.begin_pos) == slc.line && ps.column_of(start_loc.begin_pos) == slc.column)
    {
        String::new()
    } else {
        format!(" or {}", format_source_line_column(slc))
    }
}

fn format_source_line_column(slc: &SourceLineColumn) -> String {
    format!("`{}` at {}, {}", slc.source, slc.line, slc.column)
}

fn compute_start_col(ps: &ProcessedSource<'_>, style: &str, ancestor_node: Node<'_>, node: Node<'_>) -> usize {
    if style == "start_of_block" {
        let do_loc = node.loc().begin.unwrap();
        return first_non_blank(source_line(ps, do_line_anchor_loc(ps, node, do_loc))).unwrap_or(0);
    }
    ps.column_of(ancestor_node.source_range().begin_pos)
}

fn do_line_anchor_loc(ps: &ProcessedSource<'_>, node: Node<'_>, do_loc: Range) -> Range {
    if do_line_begins_inside_argument(ps, node, do_loc) {
        let send_node = node.send_node().unwrap();
        send_node.selector().unwrap_or(send_node.source_range())
    } else {
        do_loc
    }
}

fn do_line_begins_inside_argument(ps: &ProcessedSource<'_>, node: Node<'_>, do_loc: Range) -> bool {
    let line = source_line(ps, do_loc);
    let line_begin_pos = ps.line_start(ps.line_of(do_loc.begin_pos));
    let Some(offset) = line.find(|c: char| !c.is_whitespace()) else {
        return false;
    };
    let first_char_pos = line_begin_pos + offset;
    if !inside_parentheses(ps, node, first_char_pos) {
        return false;
    }
    let Some(send_node) = node.send_node() else {
        return false;
    };
    send_node
        .arguments()
        .into_iter()
        .chain(node.arguments())
        .any(|argument| {
            let range = argument.source_range();
            range.begin_pos <= first_char_pos && first_char_pos < range.end_pos
        })
}

fn inside_parentheses(ps: &ProcessedSource<'_>, node: Node<'_>, pos: usize) -> bool {
    let begin = node.source_range().begin_pos;
    let preceding = ps
        .tokens()
        .iter()
        .filter(|token| token.begin_pos() >= begin && token.begin_pos() < pos);
    let (mut opens, mut closes) = (0, 0);
    for token in preceding {
        if token.is_left_parens() || token.is_left_bracket() {
            opens += 1;
        }
        if token.is_right_parens() || token.is_right_bracket() {
            closes += 1;
        }
    }
    opens > closes
}
