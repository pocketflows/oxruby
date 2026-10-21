//! Port of `lib/rubocop/cop/layout/closing_heredoc_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::heredoc::{HEREDOC_NODE_TYPES, heredoc_node, heredoc_type};
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

const SIMPLE_HEREDOC: &str = "<<";

#[derive(Default)]
pub struct ClosingHeredocIndentation;

impl Cop for ClosingHeredocIndentation {
    fn name(&self) -> &'static str {
        "Layout/ClosingHeredocIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        HEREDOC_NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = heredoc_node(node) else {
            return;
        };
        let ps = cx.processed_source;
        if heredoc_type(node.source()) == SIMPLE_HEREDOC
            || opening_indentation(ps, node) == closing_indentation(ps, node)
            || argument_indentation_correct(ps, node)
        {
            return;
        }
        let message = message(ps, node);
        let heredoc_end = node.loc().heredoc_end.unwrap();
        cx.add_offense_with_correction(heredoc_end, &message, |corrector| {
            corrector.replace(heredoc_end, indented_end(ps, node));
        });
    }
}

fn opening_indentation(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    indent_level(heredoc_opening(ps, node))
}

fn argument_indentation_correct(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    if !(node.is_argument() || node.is_chained()) {
        return false;
    }
    opening_indentation(ps, find_node_used_heredoc_argument(node.parent().unwrap()))
        == closing_indentation(ps, node)
}

fn closing_indentation(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    indent_level(heredoc_closing(ps, node))
}

fn heredoc_opening<'s>(ps: &ProcessedSource<'s>, node: Node<'_>) -> &'s str {
    ps.source_line(node.first_line())
}

fn heredoc_closing<'s>(ps: &ProcessedSource<'s>, node: Node<'_>) -> &'s str {
    ps.source_line(ps.line_of(node.loc().heredoc_end.unwrap().begin_pos))
}

fn indented_end(ps: &ProcessedSource<'_>, node: Node<'_>) -> String {
    let closing_indent = closing_indentation(ps, node);
    let opening_indent = opening_indentation(ps, node);
    let closing_text = heredoc_closing(ps, node);
    // `gsub(/^\s{closing_indent}/, ' ' * opening_indent)` on one line.
    let prefix: String = closing_text.chars().take(closing_indent).collect();
    if prefix.chars().count() == closing_indent && prefix.chars().all(char::is_whitespace) {
        format!("{}{}", " ".repeat(opening_indent), &closing_text[prefix.len()..])
    } else {
        closing_text.to_owned()
    }
}

fn find_node_used_heredoc_argument(node: Node<'_>) -> Node<'_> {
    match node.parent() {
        Some(parent) if parent.is_send() => find_node_used_heredoc_argument(parent),
        _ => node,
    }
}

fn message(ps: &ProcessedSource<'_>, node: Node<'_>) -> String {
    let closing = heredoc_closing(ps, node).trim();
    let opening = heredoc_opening(ps, node).trim();
    if node.is_argument() {
        format!(
            "`{closing}` is not aligned with `{opening}` or beginning of method definition."
        )
    } else {
        format!("`{closing}` is not aligned with `{opening}`.")
    }
}

fn indent_level(source_line: &str) -> usize {
    source_line.bytes().take_while(|b| *b == b' ').count()
}
