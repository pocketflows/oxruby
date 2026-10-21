//! Port of `lib/rubocop/cop/layout/line_continuation_spacing.rb`.

use crate::ast::NodeType;
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

#[derive(Default)]
pub struct LineContinuationSpacing {
    /// `@ignored_ranges`, memoized for the cop's lifetime as in Ruby.
    ignored_ranges: Option<Vec<Range>>,
}

impl Cop for LineContinuationSpacing {
    fn name(&self) -> &'static str {
        "Layout/LineContinuationSpacing"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if !ps.raw_source().contains('\\') {
            return;
        }
        let last_line = last_line(cx);
        for (index, line) in ps.raw_source().split_inclusive('\n').enumerate() {
            if index >= last_line {
                break;
            }
            self.investigate(cx, line, index + 1);
        }
    }
}

impl LineContinuationSpacing {
    fn investigate(&mut self, cx: &mut Investigation<'_, '_>, line: &str, line_number: usize) {
        let Some(offensive_spacing) = find_offensive_spacing(cx, line) else {
            return;
        };
        let length = offensive_spacing.chars().count();
        let Some(column) = line.chars().count().checked_sub(length + 1) else {
            return;
        };
        let range = source_range(cx.processed_source, line_number, column, length);
        if self.ignore_range(cx, range) {
            return;
        }
        let message = if no_space_style(cx) {
            "Use zero spaces in front of backslash."
        } else {
            "Use one space in front of backslash."
        };
        let correction = if no_space_style(cx) { "\\" } else { " \\" };
        cx.add_offense_with_correction(range, message, |corrector| {
            corrector.replace(range, correction);
        });
    }

    fn ignore_range(&mut self, cx: &Investigation<'_, '_>, backtick_range: Range) -> bool {
        let ranges = self
            .ignored_ranges
            .get_or_insert_with(|| ignored_ranges(cx));
        ranges.iter().any(|range| range.contains(backtick_range))
    }
}

fn find_offensive_spacing<'l>(cx: &Investigation<'_, '_>, line: &'l str) -> Option<&'l str> {
    let re = if no_space_style(cx) {
        regex!(r"\s+\\$")
    } else if space_style(cx) {
        regex!(r"((?<!\s)|\s{2,})\\$")
    } else {
        return None;
    };
    re.find(line).map(|(begin, end)| &line[begin..end])
}

fn ignored_ranges(cx: &Investigation<'_, '_>) -> Vec<Range> {
    let mut ranges = Vec::new();
    if let Some(ast) = cx.processed_source.ast() {
        for literal in ast.each_node() {
            if !matches!(literal.ty(), NodeType::Str | NodeType::Dstr | NodeType::Array) {
                continue;
            }
            let loc = literal.loc();
            let range = if literal.is(NodeType::Array) {
                if !literal.is_percent_literal() {
                    continue;
                }
                loc.expression
            } else if literal.is_heredoc() {
                loc.heredoc_body
            } else if loc.begin.is_some() || ignored_parent(literal) {
                loc.expression
            } else {
                None
            };
            if let Some(range) = range
                && !ranges.contains(&range)
            {
                ranges.push(range);
            }
        }
    }
    ranges.extend(cx.processed_source.comments().iter().map(|c| c.range));
    ranges
}

fn last_line(cx: &Investigation<'_, '_>) -> usize {
    let ps = cx.processed_source;
    match ps.tokens().last() {
        Some(last_token) => last_token.line(ps),
        None => ps.lines().len(),
    }
}

fn ignored_parent(node: crate::ast::Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| matches!(p.ty(), NodeType::Regexp | NodeType::Xstr))
}

fn no_space_style(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyle") == Some("no_space")
}

fn space_style(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyle") == Some("space")
}
