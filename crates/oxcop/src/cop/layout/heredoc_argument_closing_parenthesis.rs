//! Port of `lib/rubocop/cop/layout/heredoc_argument_closing_parenthesis.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::ProcessedSource;
use crate::regex;

const MSG: &str = "Put the closing parenthesis for a method call with a \
                   HEREDOC parameter on the same line as the HEREDOC opening.";

#[derive(Default)]
pub struct HeredocArgumentClosingParenthesis;

impl Cop for HeredocArgumentClosingParenthesis {
    fn name(&self) -> &'static str {
        "Layout/HeredocArgumentClosingParenthesis"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/TrailingCommaInArguments"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(heredoc_arg) = extract_heredoc_argument(node) else {
            return;
        };
        let ps = cx.processed_source;
        let Some(outermost_send) = outermost_send_on_same_line(heredoc_arg) else {
            return;
        };
        if end_keyword_before_closing_parenthesis(node)
            || subsequent_closing_parentheses_in_same_line(ps, outermost_send)
            || exist_argument_between_heredoc_end_and_closing_parentheses(ps, node)
        {
            return;
        }
        cx.add_offense_with_correction(outermost_send.loc().end.unwrap(), MSG, |corrector| {
            autocorrect(ps, corrector, outermost_send);
        });
    }
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    fix_closing_parenthesis(ps, node, corrector);
    if let Some(offset) = internal_trailing_comma_offset_from_last_arg(ps, node) {
        let last_arg_end_pos = node.child_nodes().last().unwrap().source_range().end_pos;
        corrector.remove(range_between(last_arg_end_pos, last_arg_end_pos + offset));
    }
    if external_trailing_comma_offset_from_loc_end(ps, node).is_some() {
        fix_external_trailing_comma(ps, node, corrector);
    }
}

fn outermost_send_on_same_line(heredoc: Node<'_>) -> Option<Node<'_>> {
    let mut previous = heredoc;
    let mut current = previous.parent();
    while !send_missing_closing_parens(current, previous, heredoc) {
        previous = current?;
        current = previous.parent();
        current?;
    }
    current
}

fn send_missing_closing_parens(parent: Option<Node<'_>>, child: Node<'_>, heredoc: Node<'_>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    parent.is_call()
        && parent.arguments().iter().any(|a| a.same_as(child))
        && parent.loc().begin.is_some()
        && parent
            .loc()
            .end
            .is_some_and(|end| parent.line_of(end) != heredoc.last_line())
}

fn extract_heredoc_argument(node: Node<'_>) -> Option<Node<'_>> {
    node.arguments()
        .into_iter()
        .find(|arg| extract_heredoc(*arg).is_some())
}

fn extract_heredoc(node: Node<'_>) -> Option<Node<'_>> {
    if heredoc_node(Some(node)) {
        return Some(node);
    }
    if single_line_send_with_heredoc_receiver(node) {
        return node.receiver();
    }
    if !node.is(NodeType::Hash) {
        return None;
    }
    node.hash_values().into_iter().find_map(extract_heredoc)
}

fn heredoc_node(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| n.is_any_str() && n.is_heredoc())
}

fn single_line_send_with_heredoc_receiver(node: Node<'_>) -> bool {
    if !node.is_send() || !heredoc_node(node.receiver()) {
        return false;
    }
    node.receiver().unwrap().loc().heredoc_end.unwrap().end_pos > node.source_range().end_pos
}

fn end_keyword_before_closing_parenthesis(node: Node<'_>) -> bool {
    node.ancestors()
        .any(|ancestor| ancestor.loc_is(ancestor.loc().end, "end"))
}

fn subsequent_closing_parentheses_in_same_line(ps: &ProcessedSource<'_>, outermost_send: Node<'_>) -> bool {
    let Some(end_of_last_arg) = outermost_send.last_argument().and_then(|a| a.loc().end) else {
        return false;
    };
    let end_of_outer_send = outermost_send.loc().end.unwrap();
    ps.line_of(end_of_outer_send.begin_pos) == ps.line_of(end_of_last_arg.begin_pos)
        && ps.column_of(end_of_outer_send.begin_pos) == ps.column_of(end_of_last_arg.begin_pos) + 1
}

fn fix_closing_parenthesis(ps: &ProcessedSource<'_>, node: Node<'_>, corrector: &mut Corrector) {
    corrector.remove(range_between(
        incorrect_parenthesis_removal_begin(ps, node),
        incorrect_parenthesis_removal_end(ps, node),
    ));
    corrector.insert_after(node.last_argument().unwrap().source_range(), ")");
}

fn incorrect_parenthesis_removal_begin(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let end_pos = node.source_range().end_pos;
    if safe_to_remove_line_containing_closing_paren(ps, node) {
        let source = node.source();
        let last_line_length = source.rfind('\n').map_or(0, |i| {
            let rest = &source[i + 1..];
            rest.find('\n').map_or(rest.len(), |j| j)
        });
        end_pos - last_line_length - 1
    } else {
        end_pos - 1
    }
}

fn safe_to_remove_line_containing_closing_paren(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let line = ps.line_of(node.loc().end.unwrap().begin_pos);
    let last_line = ps.lines()[line - 1];
    regex!(r"^ *\) {0,20},{0,1} *$").find(last_line).is_some()
}

fn incorrect_parenthesis_removal_end(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let end_pos = node.source_range().end_pos;
    if ps.buffer_source().as_bytes().get(end_pos) == Some(&b',') {
        end_pos + 1
    } else {
        end_pos
    }
}

fn exist_argument_between_heredoc_end_and_closing_parentheses(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
) -> bool {
    let Some(end) = node.loc().end else {
        return true;
    };
    let Some(heredoc_end) = node
        .arguments()
        .iter()
        .filter_map(|argument| argument.loc().heredoc_end.map(|h| h.end_pos))
        .max()
    else {
        return false;
    };
    heredoc_end < end.begin_pos && !ps.slice(range_between(heredoc_end, end.begin_pos)).trim().is_empty()
}

/// Returns `None` if no trailing internal comma.
fn internal_trailing_comma_offset_from_last_arg(ps: &ProcessedSource<'_>, node: Node<'_>) -> Option<usize> {
    let source_after_last_arg = ps.slice(range_between(
        node.child_nodes().last()?.source_range().end_pos,
        node.loc().end?.begin_pos,
    ));
    let first_comma_offset = source_after_last_arg.find(',')?;
    let first_new_line_offset = source_after_last_arg.find('\n')?;
    if first_comma_offset > first_new_line_offset {
        return None;
    }
    Some(first_comma_offset + 1)
}

fn fix_external_trailing_comma(ps: &ProcessedSource<'_>, node: Node<'_>, corrector: &mut Corrector) {
    let end_pos = node.source_range().end_pos;
    let Some(offset) = external_trailing_comma_offset_from_loc_end(ps, node) else {
        return;
    };
    corrector.remove(range_between(end_pos, end_pos + offset));
    corrector.insert_after(node.last_argument().unwrap().source_range(), ",");
}

/// Returns `None` if no trailing external comma.
fn external_trailing_comma_offset_from_loc_end(ps: &ProcessedSource<'_>, node: Node<'_>) -> Option<usize> {
    let source = ps.buffer_source().as_bytes();
    let end_pos = node.source_range().end_pos;
    let mut offset = 0;
    let limit = 20;
    while offset < limit && source.get(end_pos + offset) == Some(&b' ') {
        offset += 1;
    }
    (source.get(end_pos + offset) == Some(&b',')).then_some(offset + 1)
}
