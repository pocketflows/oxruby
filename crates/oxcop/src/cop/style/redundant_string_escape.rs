//! Port of `lib/rubocop/cop/style/redundant_string_escape.rb`.

use crate::ast::{MapKind, Node, NodeType};
use crate::cop::mixin::match_range::each_match_range;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

#[derive(Default)]
pub struct RedundantStringEscape;

impl Cop for RedundantStringEscape {
    fn name(&self) -> &'static str {
        "Style/RedundantStringEscape"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node
            .parent()
            .is_some_and(|p| matches!(p.ty(), NodeType::Regexp | NodeType::Xstr))
            || node.is_character_literal()
        {
            return;
        }
        let range = if heredoc(node) {
            node.loc().heredoc_body.unwrap()
        } else {
            node.source_range()
        };
        let src = cx.processed_source.buffer_source();
        for range in each_match_range(range, &src[range.begin_pos..range.end_pos], regex!(r"(\\.)")) {
            if allowed_escape(src, node, range) {
                continue;
            }
            let escaped = src[range.begin_pos..range.end_pos].chars().last().unwrap_or(' ');
            let message = format!("Redundant escape of {escaped} inside string literal.");
            cx.add_offense_with_correction(range, &message, |corrector| {
                corrector.remove_leading(range, 1);
            });
        }
    }
}

/// The byte position `chars` characters after (or, negative, before) `pos`.
fn char_pos(src: &str, pos: usize, chars: isize) -> usize {
    if chars >= 0 {
        src[pos..]
            .char_indices()
            .nth(chars as usize)
            .map_or(src.len(), |(i, _)| pos + i)
    } else {
        src[..pos]
            .char_indices()
            .rev()
            .nth((-chars - 1) as usize)
            .map_or(0, |(i, _)| i)
    }
}

/// `allowed_escape?(node, range.resize(3))`; `range` is the `\x` match.
fn allowed_escape(src: &str, node: Node<'_>, range: Range) -> bool {
    let range3 = Range::new(range.begin_pos, char_pos(src, range.begin_pos, 3));
    let escaped = &src[char_pos(src, range3.begin_pos, 1)..range3.end_pos];
    let first = escaped.chars().next().unwrap_or('\0');

    if interpolation_not_enabled(node) {
        return true;
    }
    if first == '\n' || first == '\\' || first.is_alphanumeric() {
        return true;
    }
    if first == ' ' && (percent_array_literal(node) || node.is_heredoc()) {
        return true;
    }
    if disabling_interpolation(src, range3) {
        return true;
    }
    delimiter(node, first)
}

fn begin_loc_present(node: Node<'_>) -> bool {
    node.loc().kind == MapKind::Collection && node.loc().begin.is_some()
}

fn interpolation_not_enabled(node: Node<'_>) -> bool {
    delimiter(node, '\'')
        || array_literal(node, "%w")
        || percent_q_literal(node)
        || heredoc_with_disabled_interpolation(node)
}

fn literal_in_interpolated_or_multiline_string(node: Node<'_>) -> bool {
    node.is(NodeType::Str)
        && !begin_loc_present(node)
        && node.parent().is_some_and(|p| p.is(NodeType::Dstr))
}

fn percent_q_literal(node: Node<'_>) -> bool {
    if literal_in_interpolated_or_multiline_string(node) {
        percent_q_literal(node.parent().unwrap())
    } else {
        node.source().starts_with("%q")
    }
}

fn array_literal(node: Node<'_>, prefix: &str) -> bool {
    if literal_in_interpolated_or_multiline_string(node) {
        array_literal(node.parent().unwrap(), prefix)
    } else {
        node.parent()
            .is_some_and(|p| p.is(NodeType::Array) && p.source().starts_with(prefix))
    }
}

fn percent_array_literal(node: Node<'_>) -> bool {
    array_literal(node, "%w") || array_literal(node, "%W")
}

fn heredoc_with_disabled_interpolation(node: Node<'_>) -> bool {
    if heredoc(node) {
        node.source().ends_with('\'')
    } else if let Some(parent) = node.parent().filter(|p| p.is(NodeType::Dstr)) {
        heredoc_with_disabled_interpolation(parent)
    } else {
        false
    }
}

fn heredoc(node: Node<'_>) -> bool {
    matches!(node.ty(), NodeType::Str | NodeType::Dstr) && node.is_heredoc()
}

fn delimiter(node: Node<'_>, ch: char) -> bool {
    if heredoc(node) {
        return false;
    }
    if literal_in_interpolated_or_multiline_string(node) || percent_array_literal(node) {
        return delimiter(node.parent().unwrap(), ch);
    }
    let (Some(begin), Some(end)) = (node.loc().begin, node.loc().end) else {
        return true;
    };
    node.range_source(begin).ends_with(ch) || node.range_source(end).starts_with(ch)
}

fn disabling_interpolation(src: &str, range: Range) -> bool {
    // Allow \#{foo}, \#$foo, \#@foo, and \#@@foo
    if regex!(r"\A\\#[{$@]")
        .find(&src[range.begin_pos..range.end_pos])
        .is_some()
    {
        return true;
    }
    // Also allow #\{foo}, #\$foo, #\@foo and #\@@foo
    let before = char_pos(src, range.begin_pos, -2);
    if regex!(r"\A[^\\]#\\[{$@]")
        .find(&src[before..range.end_pos])
        .is_some()
    {
        return true;
    }
    // For `\#\{foo} allow `\#` and warn `\{`
    &src[range.begin_pos..char_pos(src, range.end_pos, 1)] == "\\#\\{"
}
