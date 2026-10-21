//! Port of `lib/rubocop/cop/layout/space_inside_block_braces.rb`. The
//! style-detection calls only feed `--auto-gen-config`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_between, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct SpaceInsideBlockBraces;

impl Cop for SpaceInsideBlockBraces {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideBlockBraces"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/BlockDelimiters"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_keywords() {
            return;
        }
        if node.body().is_none() && node.is_multiline() {
            return;
        }
        let left_brace = node.loc().begin.unwrap();
        let right_brace = node.loc().end.unwrap();
        check_inside(cx, node, left_brace, right_brace);
    }
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c')
}

fn check_inside(cx: &mut Investigation<'_, '_>, node: Node<'_>, left_brace: Range, right_brace: Range) {
    if left_brace.end_pos == right_brace.begin_pos {
        adjacent_braces(cx, left_brace, right_brace);
    } else {
        let range = range_between(left_brace.end_pos, right_brace.begin_pos);
        let inner = cx.source(range);
        if inner.chars().any(|c| !is_space(c)) {
            braces_with_contents_inside(cx, node, inner);
        } else if style_for_empty_braces(cx) == "no_space" {
            offense(cx, range.begin_pos, range.end_pos, "Space inside empty braces detected.");
        }
    }
}

fn adjacent_braces(cx: &mut Investigation<'_, '_>, left_brace: Range, right_brace: Range) {
    if style_for_empty_braces(cx) != "space" {
        return;
    }
    offense(
        cx,
        left_brace.begin_pos,
        right_brace.end_pos,
        "Space missing inside empty braces.",
    );
}

fn braces_with_contents_inside(cx: &mut Investigation<'_, '_>, node: Node<'_>, inner: &str) {
    let args_delimiter = if node.is(NodeType::Block) {
        node.arguments_node().and_then(|args| args.loc().begin)
    } else {
        None
    };
    let left_brace = node.loc().begin.unwrap();
    check_left_brace(cx, inner, left_brace, args_delimiter);
    check_right_brace(cx, node, inner, left_brace, node.loc().end.unwrap(), node.is_single_line());
}

fn check_left_brace(cx: &mut Investigation<'_, '_>, inner: &str, left_brace: Range, args_delimiter: Option<Range>) {
    if inner.chars().next().is_some_and(|c| !is_space(c)) {
        no_space_inside_left_brace(cx, left_brace, args_delimiter);
    } else {
        space_inside_left_brace(cx, left_brace, args_delimiter);
    }
}

fn check_right_brace(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    inner: &str,
    left_brace: Range,
    right_brace: Range,
    single_line: bool,
) {
    // `/\S$/`: a non-blank character at the end of a line.
    let ends_with_non_space = inner
        .split('\n')
        .any(|line| line.chars().next_back().is_some_and(|c| !is_space(c)));
    if single_line && ends_with_non_space {
        no_space(cx, right_brace.begin_pos, right_brace.end_pos, "Space missing inside }.");
    } else {
        let column = node.column();
        if multiline_block(cx, left_brace, right_brace) && aligned_braces(cx, inner, right_brace, column) {
            return;
        }
        space_inside_right_brace(cx, inner, right_brace, column);
    }
}

fn multiline_block(cx: &Investigation<'_, '_>, left_brace: Range, right_brace: Range) -> bool {
    let ps = cx.processed_source;
    ps.line_of(left_brace.begin_pos) != ps.line_of(right_brace.begin_pos)
}

fn aligned_braces(cx: &Investigation<'_, '_>, inner: &str, right_brace: Range, column: usize) -> bool {
    column == cx.processed_source.column_of(right_brace.begin_pos) || column == inner_last_space_count(inner)
}

/// `inner.split("\n").last.count(' ')` (Ruby's `split` drops trailing empty
/// fields).
fn inner_last_space_count(inner: &str) -> usize {
    let mut fields: Vec<&str> = inner.split('\n').collect();
    while fields.last().is_some_and(|f| f.is_empty()) {
        fields.pop();
    }
    fields.last().map_or(0, |f| f.matches(' ').count())
}

fn pipe(cx: &Investigation<'_, '_>, args_delimiter: Option<Range>) -> Option<Range> {
    args_delimiter.filter(|d| cx.source(*d) == "|")
}

fn no_space_inside_left_brace(cx: &mut Investigation<'_, '_>, left_brace: Range, args_delimiter: Option<Range>) {
    if let Some(args_delimiter) = pipe(cx, args_delimiter) {
        if left_brace.end_pos == args_delimiter.begin_pos
            && cx.cop_config_bool("SpaceBeforeBlockParameters", false)
        {
            offense(cx, left_brace.begin_pos, args_delimiter.end_pos, "Space between { and | missing.");
        }
    } else {
        no_space(cx, left_brace.end_pos, left_brace.end_pos + 1, "Space missing inside {.");
    }
}

fn space_inside_left_brace(cx: &mut Investigation<'_, '_>, left_brace: Range, args_delimiter: Option<Range>) {
    if let Some(args_delimiter) = pipe(cx, args_delimiter) {
        if !cx.cop_config_bool("SpaceBeforeBlockParameters", false) {
            offense(cx, left_brace.end_pos, args_delimiter.begin_pos, "Space between { and | detected.");
        }
    } else {
        let brace_with_space = range_with_surrounding_space(
            cx.processed_source.buffer_source(),
            left_brace,
            SurroundingSpace::side(Side::Right),
        );
        space(cx, brace_with_space.begin_pos + 1, brace_with_space.end_pos, "Space inside { detected.");
    }
}

fn space_inside_right_brace(cx: &mut Investigation<'_, '_>, inner: &str, right_brace: Range, column: usize) {
    let brace_with_space = range_with_surrounding_space(
        cx.processed_source.buffer_source(),
        right_brace,
        SurroundingSpace::side(Side::Left),
    );
    let mut begin_pos = brace_with_space.begin_pos as isize;
    let mut end_pos = brace_with_space.end_pos as isize - 1;
    if cx.source(brace_with_space).contains(['\n', '\r']) {
        let right_column = cx.processed_source.column_of(right_brace.begin_pos) as isize;
        begin_pos = end_pos - (right_column - column as isize);
    }
    if inner.ends_with(']') {
        end_pos -= 1;
        begin_pos = end_pos - (inner_last_space_count(inner) as isize - column as isize);
    }
    if begin_pos < 0 || end_pos < 0 {
        return;
    }
    space(cx, begin_pos as usize, end_pos as usize, "Space inside } detected.");
}

fn no_space(cx: &mut Investigation<'_, '_>, begin_pos: usize, end_pos: usize, msg: &str) {
    if style(cx) == "space" {
        offense(cx, begin_pos, end_pos, msg);
    }
}

fn space(cx: &mut Investigation<'_, '_>, begin_pos: usize, end_pos: usize, msg: &str) {
    if style(cx) == "no_space" {
        offense(cx, begin_pos, end_pos, msg);
    }
}

fn offense(cx: &mut Investigation<'_, '_>, begin_pos: usize, end_pos: usize, msg: &str) {
    if begin_pos > end_pos {
        return;
    }
    let range = range_between(begin_pos, end_pos);
    let source = cx.source(range);
    cx.add_offense_with_correction(range, msg, |corrector| {
        if source.chars().any(is_space) {
            corrector.remove(range);
        } else if source == "{}" {
            corrector.replace(range, "{ }");
        } else if source == "{|" {
            corrector.replace(range, "{ |");
        } else {
            corrector.insert_before(range, " ");
        }
    });
}

fn style_for_empty_braces(cx: &Investigation<'_, '_>) -> &'static str {
    match cx.cop_config_str("EnforcedStyleForEmptyBraces") {
        Some("space") => "space",
        Some("no_space") => "no_space",
        _ => panic!("Unknown EnforcedStyleForEmptyBraces selected!"),
    }
}
