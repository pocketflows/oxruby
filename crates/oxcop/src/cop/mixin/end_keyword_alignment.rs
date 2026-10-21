//! Port of `lib/rubocop/cop/mixin/end_keyword_alignment.rb`.
//!
//! The style parameter is `EnforcedStyleAlignWith`. The cop's ignored nodes
//! and its `autocorrect(corrector, node)` are passed in.

use crate::ast::Node;
use crate::cop::mixin::configurable_enforced_style::style_for;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::{column_offset_between, range_between};
use crate::cop::{Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

pub const STYLE_PARAMETER_NAME: &str = "EnforcedStyleAlignWith";

/// `style` (with `style_parameter_name` `EnforcedStyleAlignWith`).
pub fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    style_for(cx, STYLE_PARAMETER_NAME)
}

/// `check_end_kw_in_node(node)`.
pub fn check_end_kw_in_node<'a>(
    cx: &mut Investigation<'a, '_>,
    ignored: &IgnoredNodes,
    node: Node<'a>,
    autocorrect: impl FnOnce(&mut Corrector, &Investigation<'a, '_>),
) {
    let style = style(cx);
    let Some(keyword) = node.loc().keyword else {
        return;
    };
    check_end_kw_alignment(cx, ignored, node, &[(style, keyword)], autocorrect);
}

/// `check_end_kw_alignment(node, align_ranges)`: `align_ranges` maps style
/// names to ranges, in the Ruby hash's order.
pub fn check_end_kw_alignment<'a>(
    cx: &mut Investigation<'a, '_>,
    ignored: &IgnoredNodes,
    node: Node<'a>,
    align_ranges: &[(&str, Range)],
    autocorrect: impl FnOnce(&mut Corrector, &Investigation<'a, '_>),
) {
    if ignored.contains(node) {
        return;
    }
    let Some(end_loc) = node.loc().end else {
        return;
    };
    let style = style(cx);
    let ps = cx.processed_source;
    let matching = matching_ranges(ps, end_loc, align_ranges);
    if matching.contains(&style) {
        return;
    }
    let Some(&(_, align_with)) = align_ranges.iter().find(|(s, _)| *s == style) else {
        return;
    };
    add_offense_for_misalignment(cx, node, align_with, autocorrect);
}

fn matching_ranges<'r>(
    ps: &ProcessedSource<'_>,
    end_loc: Range,
    align_ranges: &[(&'r str, Range)],
) -> Vec<&'r str> {
    align_ranges
        .iter()
        .filter(|(_, range)| {
            ps.line_of(range.begin_pos) == ps.line_of(end_loc.begin_pos)
                || column_offset_between(ps, *range, end_loc) == 0
        })
        .map(|(style, _)| *style)
        .collect()
}

/// `start_line_range(node)`: the node's first line, from its first
/// non-blank character to the end of its non-blank text.
pub fn start_line_range(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let line = node.first_line();
    let source = ps.source_line(line);
    let range = ps.line_range(line);
    let begin = regex!(r"\S").find(source).map_or(0, |(b, _)| b);
    let end = regex!(r"\s*\z")
        .find(source)
        .map_or(source.len(), |(b, _)| b);
    range_between(range.begin_pos + begin, range.begin_pos + end)
}

fn add_offense_for_misalignment<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    align_with: Range,
    autocorrect: impl FnOnce(&mut Corrector, &Investigation<'a, '_>),
) {
    let ps = cx.processed_source;
    let end_loc = node.loc().end.unwrap();
    let msg = format!(
        "`end` at {}, {} is not aligned with `{}` at {}, {}.",
        ps.line_of(end_loc.begin_pos),
        ps.column_of(end_loc.begin_pos),
        ps.slice(align_with),
        ps.line_of(align_with.begin_pos),
        ps.column_of(align_with.begin_pos)
    );
    cx.add_offense_cx(end_loc, &msg, autocorrect);
}

/// `variable_alignment?(whole_expression, rhs, end_alignment_style)`.
pub fn variable_alignment(
    ps: &ProcessedSource<'_>,
    whole_expression: Range,
    rhs: Node<'_>,
    end_alignment_style: &str,
) -> bool {
    if end_alignment_style == "keyword" {
        return false;
    }
    !line_break_before_keyword(ps, whole_expression, rhs)
}

/// `line_break_before_keyword?(whole_expression, rhs)`.
pub fn line_break_before_keyword(
    ps: &ProcessedSource<'_>,
    whole_expression: Range,
    rhs: Node<'_>,
) -> bool {
    rhs.first_line() > ps.line_of(whole_expression.begin_pos)
}
