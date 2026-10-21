//! Port of `lib/rubocop/cop/mixin/multiline_element_indentation.rb`.
//!
//! The including cop's `style`, `brace_alignment_style`, `message` (from a
//! base description) and `autocorrect` are passed in. `@column_delta` is
//! handed to the correction. Style detection only feeds
//! `--auto-gen-config` and is not ported.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::{Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

/// `indent_base_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentBase {
    LeftBraceOrBracket,
    ParentHashKey,
    FirstColumnAfterLeftParenthesis,
    StartOfLine,
}

/// The including cop's configuration.
#[derive(Debug, Clone, Copy)]
pub struct Settings<'s> {
    pub style: &'s str,
    pub brace_alignment_style: &'static str,
}

/// A cop's `message(base_description(type))`.
pub type MessageFn<'f> = dyn Fn(&Investigation<'_, '_>, IndentBase) -> String + 'f;

/// A cop's `autocorrect(corrector, node)` with `@column_delta`.
pub type AutocorrectFn<'f, 'a> =
    dyn Fn(&mut Corrector, &Investigation<'_, '_>, Node<'a>, isize) + 'f;

/// `each_argument_node(node, type)`: the `type` nodes among the arguments
/// (not looking inside nested calls) whose opening brace is on the line of
/// the call's `(`, with that `(`. The caller ignores each after checking it.
pub fn each_argument_node<'a>(
    ps: &ProcessedSource<'_>,
    node: Node<'a>,
    ty: NodeType,
) -> Vec<(Node<'a>, Range)> {
    let Some(left_parenthesis) = node.loc().begin else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for arg in node.arguments() {
        on_node(ty, arg, &mut |type_node| {
            if let Some(left_brace) = type_node.loc().begin
                && ps.line_of(left_brace.begin_pos) == ps.line_of(left_parenthesis.begin_pos)
            {
                found.push((type_node, left_parenthesis));
            }
        });
    }
    found
}

/// `Util#on_node(type, sexp, :send)`.
fn on_node<'a>(ty: NodeType, sexp: Node<'a>, f: &mut dyn FnMut(Node<'a>)) {
    if sexp.is(ty) {
        f(sexp);
    }
    if sexp.is(NodeType::Send) {
        return;
    }
    for child in sexp.child_nodes() {
        on_node(ty, child, f);
    }
}

/// `check_first(first, left_brace, left_parenthesis, offset)`.
#[allow(clippy::too_many_arguments)]
pub fn check_first<'a>(
    cx: &mut Investigation<'_, '_>,
    settings: Settings<'_>,
    first: Node<'a>,
    left_brace: Range,
    left_parenthesis: Option<Range>,
    offset: isize,
    message: &MessageFn<'_>,
    autocorrect: &AutocorrectFn<'_, 'a>,
) {
    let ps = cx.processed_source;
    let actual_column = first.column() as isize;
    let (indent_base_column, indent_base_type) =
        indent_base(ps, settings, left_brace, Some(first), left_parenthesis);
    let expected_column =
        indent_base_column as isize + configured_indentation_width(cx) as isize + offset;
    let column_delta = expected_column - actual_column;
    if column_delta == 0 {
        return;
    }
    let msg = message(cx, indent_base_type);
    cx.add_offense_cx(first.source_range(), &msg, |corrector, cx| {
        autocorrect(corrector, cx, first, column_delta);
    });
}

/// `indent_base(left_brace, first, left_parenthesis)`.
pub fn indent_base(
    ps: &ProcessedSource<'_>,
    settings: Settings<'_>,
    left_brace: Range,
    first: Option<Node<'_>>,
    left_parenthesis: Option<Range>,
) -> (usize, IndentBase) {
    if settings.style == settings.brace_alignment_style {
        return (
            ps.column_of(left_brace.begin_pos),
            IndentBase::LeftBraceOrBracket,
        );
    }
    if let Some(pair) = hash_pair_where_value_beginning_with(left_brace, first)
        && key_and_value_begin_on_same_line(pair)
        && right_sibling_begins_on_subsequent_line(pair)
    {
        return (pair.column(), IndentBase::ParentHashKey);
    }
    if let Some(left_parenthesis) = left_parenthesis
        && settings.style == "special_inside_parentheses"
    {
        return (
            ps.column_of(left_parenthesis.begin_pos) + 1,
            IndentBase::FirstColumnAfterLeftParenthesis,
        );
    }
    let line = ps.source_line(ps.line_of(left_brace.begin_pos));
    let column = line.chars().position(|c| !c.is_whitespace()).unwrap_or(0);
    (column, IndentBase::StartOfLine)
}

fn hash_pair_where_value_beginning_with<'a>(
    left_brace: Range,
    first: Option<Node<'a>>,
) -> Option<Node<'a>> {
    let parent = first?.parent()?;
    if parent.loc().begin != Some(left_brace) {
        return None;
    }
    parent.parent().filter(|p| p.is(NodeType::Pair))
}

fn key_and_value_begin_on_same_line(pair: Node<'_>) -> bool {
    match (pair.key(), pair.pair_value()) {
        (Some(key), Some(value)) => key.first_line() == value.first_line(),
        _ => false,
    }
}

fn right_sibling_begins_on_subsequent_line(pair: Node<'_>) -> bool {
    pair.right_sibling()
        .is_some_and(|sibling| pair.last_line() < sibling.first_line())
}
