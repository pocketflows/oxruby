//! Port of `lib/rubocop/cop/mixin/check_line_breakable.rb`. The
//! `children_could_be_broken_up?` cache is only an optimization and is not
//! ported.

use crate::ast::{Node, NodeType};
use crate::processed_source::ProcessedSource;

/// `extract_breakable_node(node, max)`.
pub fn extract_breakable_node<'a>(
    ps: &ProcessedSource<'_>,
    node: Node<'a>,
    max: Option<usize>,
) -> Option<Node<'a>> {
    if node.is_call() {
        if chained_to_heredoc(node) {
            return None;
        }
        let args = process_args(node.arguments());
        extract_breakable_node_from_elements(ps, node, &args, max)
    } else if node.is_any_def() {
        extract_breakable_node_from_elements(ps, node, &node.arguments(), max)
    } else if matches!(node.ty(), NodeType::Array | NodeType::Hash) {
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        extract_breakable_node_from_elements(ps, node, &children, max)
    } else {
        None
    }
}

fn extract_breakable_node_from_elements<'a>(
    ps: &ProcessedSource<'_>,
    node: Node<'a>,
    elements: &[Node<'a>],
    max: Option<usize>,
) -> Option<Node<'a>> {
    if !breakable_collection(node, elements) {
        return None;
    }
    let line = ps.lines()[node.first_line() - 1];
    if let Some(max) = max
        && line.chars().count() <= max
    {
        return None;
    }
    if ps.line_with_comment(node.first_line()) {
        return None;
    }
    if safe_to_ignore(node, max) {
        return None;
    }
    extract_first_element_over_column_limit(node, elements, max)
}

fn extract_first_element_over_column_limit<'a>(
    node: Node<'a>,
    elements: &[Node<'a>],
    max: Option<usize>,
) -> Option<Node<'a>> {
    let line = node.first_line();
    let elements = if node.is_call() && !node.is_parenthesized() && !first_argument_is_heredoc(node)
    {
        &elements[1.min(elements.len())..]
    } else {
        elements
    };
    let mut i = 0;
    while within_column_limit(elements.get(i).copied(), max, line) {
        i += 1;
    }
    let i = shift_elements_for_heredoc_arg(node, elements, i)?;
    if i == 0 {
        return elements.first().copied();
    }
    elements.get(i - 1).copied()
}

fn is_str_heredoc(node: Node<'_>) -> bool {
    matches!(node.ty(), NodeType::Str | NodeType::Dstr | NodeType::Xstr) && node.is_heredoc()
}

fn first_argument_is_heredoc(node: Node<'_>) -> bool {
    node.first_argument().is_some_and(is_str_heredoc)
}

fn shift_elements_for_heredoc_arg(
    node: Node<'_>,
    elements: &[Node<'_>],
    index: usize,
) -> Option<usize> {
    if !(node.is_call() || node.is(NodeType::Array)) {
        return Some(index);
    }
    let Some(heredoc_index) = elements.iter().position(|arg| is_str_heredoc(*arg)) else {
        return Some(index);
    };
    if heredoc_index == 0 {
        return None;
    }
    Some(if heredoc_index >= index {
        index
    } else {
        heredoc_index + 1
    })
}

fn within_column_limit(element: Option<Node<'_>>, max: Option<usize>, line: usize) -> bool {
    let Some(element) = element else {
        return false;
    };
    // `element.loc.column <= max` (comparing with `nil` would raise).
    max.is_some_and(|max| element.column() <= max) && element.first_line() == line
}

fn safe_to_ignore(node: Node<'_>, max: Option<usize>) -> bool {
    if max.is_none() {
        return true;
    }
    already_on_multiple_lines(node)
        || contained_by_breakable_collection_on_same_line(node)
        || contained_by_multiline_collection_that_could_be_broken_up(node)
}

/// `breakable_collection?(node, elements)`.
pub fn breakable_collection(node: Node<'_>, elements: &[Node<'_>]) -> bool {
    let starts_with_bracket = !node.is(NodeType::Hash) || node.loc().begin.is_some();
    let has_second_element = elements.len() >= 2;
    starts_with_bracket && has_second_element
}

fn contained_by_breakable_collection_on_same_line(node: Node<'_>) -> bool {
    for ancestor in node.ancestors() {
        if ancestor.first_line() != node.first_line() {
            break;
        }
        let elements: Vec<Node<'_>> = if matches!(ancestor.ty(), NodeType::Hash | NodeType::Array) {
            ancestor.child_nodes().collect()
        } else if ancestor.is_call() {
            process_args(ancestor.arguments())
        } else {
            continue;
        };
        if breakable_collection(ancestor, &elements) {
            return true;
        }
    }
    false
}

fn contained_by_multiline_collection_that_could_be_broken_up(node: Node<'_>) -> bool {
    for ancestor in node.ancestors() {
        if matches!(ancestor.ty(), NodeType::Hash | NodeType::Array) {
            let children: Vec<Node<'_>> = ancestor.child_nodes().collect();
            if breakable_collection(ancestor, &children) {
                return children_could_be_broken_up(&children);
            }
        }
        if !ancestor.is_call() {
            continue;
        }
        let args = process_args(ancestor.arguments());
        if breakable_collection(ancestor, &args) {
            return children_could_be_broken_up(&args);
        }
    }
    false
}

fn children_could_be_broken_up(children: &[Node<'_>]) -> bool {
    if all_on_same_line(children) {
        return false;
    }
    let mut last_seen_line = 0;
    for child in children {
        if last_seen_line >= child.first_line() {
            return true;
        }
        last_seen_line = child.last_line();
    }
    false
}

fn all_on_same_line(nodes: &[Node<'_>]) -> bool {
    match (nodes.first(), nodes.last()) {
        (Some(first), Some(last)) => first.first_line() == last.last_line(),
        _ => true,
    }
}

/// `process_args(args)`: a braceless hash as the last argument contributes
/// its pairs.
pub fn process_args(mut args: Vec<Node<'_>>) -> Vec<Node<'_>> {
    if let Some(last_arg) = args.last().copied()
        && last_arg.is(NodeType::Hash)
        && !last_arg.has_braces()
    {
        args.pop();
        args.extend(last_arg.child_nodes());
    }
    args
}

fn already_on_multiple_lines(node: Node<'_>) -> bool {
    if node.is_any_def() {
        return node
            .last_argument()
            .is_none_or(|last| node.first_line() != last.last_line());
    }
    node.is_multiline()
}

fn chained_to_heredoc(node: Node<'_>) -> bool {
    let mut node = node;
    while let Some(receiver) = node.receiver() {
        if is_str_heredoc(receiver) {
            return true;
        }
        node = receiver;
    }
    false
}
