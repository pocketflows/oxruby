//! Port of `lib/rubocop/cop/style/if_unless_modifier.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::line_length_help::max_line_length;
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_by_whole_lines, range_with_surrounding_space,
};
use crate::cop::mixin::statement_modifier::{
    acceptable_line_length, non_eligible_node as statement_non_eligible_node, parenthesize,
    single_line_as_modifier_with, to_modifier_form,
};
use crate::cop::mixin::util::{indent, same_line};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{Comment, Range};

const MSG_USE_MODIFIER: &str = "Favor modifier `%<keyword>s` usage when having a single-line body. Another good alternative is the usage of control flow `&&`/`||`.";
const MSG_USE_MODIFIER_PARENS: &str = "Favor modifier `%<keyword>s` usage when having a single-line body. Wrap the expression in parentheses to keep the current behavior, as it is part of a larger expression.";
const MSG_USE_NORMAL: &str = "Modifier form of `%<keyword>s` makes the line too long.";

#[derive(Default)]
pub struct IfUnlessModifier {
    ignored: IgnoredNodes,
}

impl Cop for IfUnlessModifier {
    fn name(&self) -> &'static str {
        "Style/IfUnlessModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/Next", "Style/SoleNestedConditional"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if endless_method(node.body()) || node.ancestors().any(|a| a.is(NodeType::Dstr)) {
            return;
        }
        let Some(condition) = node.condition() else {
            return;
        };
        if defined_nodes(condition)
            .into_iter()
            .any(|n| defined_argument_is_undefined(node, n))
            || !pattern_matching_nodes(condition).is_empty()
        {
            return;
        }
        let Some(msg) = message(cx, node) else {
            return;
        };
        let message = msg.replace("%<keyword>s", node.keyword());
        let ignored = &mut self.ignored;
        cx.add_offense_cx(node.loc().keyword.unwrap(), &message, |corrector, cx| {
            if ignored.part_of(node) {
                return;
            }
            if another_modifier_if_on_same_line(node) {
                return;
            }
            autocorrect(cx, corrector, node);
            ignored.ignore(node);
        });
    }
}

fn endless_method(body: Option<Node<'_>>) -> bool {
    body.is_some_and(|b| b.is_any_def() && b.is_endless())
}

fn defined_nodes(condition: Node<'_>) -> Vec<Node<'_>> {
    if condition.is(NodeType::Defined) {
        vec![condition]
    } else {
        condition
            .descendants()
            .filter(|d| d.is(NodeType::Defined))
            .collect()
    }
}

fn defined_argument_is_undefined(if_node: Node<'_>, defined_node: Node<'_>) -> bool {
    let Some(defined_argument) = defined_node.child(0) else {
        return false;
    };
    if !defined_argument.is_any(&[NodeType::Lvar, NodeType::Send]) {
        return false;
    }
    // `defined_argument.node_parts[0]`: an `lvar`'s name, a `send`'s receiver
    // (never equal to a variable name).
    let name = if defined_argument.is(NodeType::Lvar) {
        defined_argument.child_sym(0)
    } else {
        None
    };
    !if_node
        .left_siblings()
        .iter()
        .any(|s| s.is(NodeType::Lvasgn) && name.is_some() && s.child_sym(0) == name)
}

fn pattern_matching_nodes(condition: Node<'_>) -> Vec<Node<'_>> {
    if condition.is_any_match_pattern() {
        vec![condition]
    } else {
        condition
            .descendants()
            .filter(|d| d.is_any_match_pattern())
            .collect()
    }
}

fn message(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<&'static str> {
    if single_line_as_modifier_with(cx, node, non_eligible_node)
        && !named_capture_in_condition(node)
    {
        Some(if parenthesize(node) {
            MSG_USE_MODIFIER_PARENS
        } else {
            MSG_USE_MODIFIER
        })
    } else if too_long_due_to_modifier(cx, node) {
        Some(MSG_USE_NORMAL)
    } else {
        None
    }
}

fn autocorrect(cx: &Investigation<'_, '_>, corrector: &mut Corrector, node: Node<'_>) {
    let replacement = if node.is_modifier_form() {
        replacement_for_modifier_form(cx, corrector, node)
    } else {
        to_modifier_form(cx, node)
    };
    corrector.replace(node.source_range(), replacement);
}

fn replacement_for_modifier_form(
    cx: &Investigation<'_, '_>,
    corrector: &mut Corrector,
    node: Node<'_>,
) -> String {
    let comment = comment_on_node_line(cx, node);
    if let Some(comment) = comment
        && too_long_due_to_comment_after_modifier(cx, node, &comment)
    {
        remove_comment(cx, corrector, &comment);
        return to_modifier_form_with_move_comment(node, &indent(node, 0), &comment);
    }

    let if_branch = node.if_branch();
    let last_argument = if_branch
        .filter(|b| b.is(NodeType::Send))
        .and_then(|b| b.last_argument());
    if let Some(last_argument) = last_argument
        && last_argument.is_any_str()
        && last_argument.is_heredoc()
    {
        let heredoc = extract_heredoc_from(last_argument);
        remove_heredoc(cx, corrector, heredoc);
        return to_normal_form_with_heredoc(cx, node, &indent(node, 0), heredoc);
    }

    to_normal_form(node, &indent(node, 0))
}

fn too_long_due_to_modifier(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is_modifier_form() && too_long_single_line(cx, node) && !another_statement_on_same_line(node)
}

fn too_long_due_to_comment_after_modifier(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    comment: &Comment<'_>,
) -> bool {
    let source_length = cx.processed_source.lines()[node.first_line() - 1]
        .chars()
        .count();
    let comment_length = comment.text.chars().count();
    let Some(max) = max_line_length(cx) else {
        return false;
    };
    source_length.saturating_sub(comment_length) <= max && max <= source_length
}

fn too_long_single_line(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let range = node.source_range();
    let ps = cx.processed_source;
    let first_line = ps.line_of(range.begin_pos);
    first_line == ps.line_of(range.end_pos)
        && !acceptable_line_length(cx, ps.source_line(first_line), first_line)
}

fn named_capture_in_condition(node: Node<'_>) -> bool {
    node.condition()
        .is_some_and(|c| c.is(NodeType::MatchWithLvasgn))
}

fn non_eligible_node(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    non_simple_if_unless(node)
        || node.is_chained()
        || node.is_nested_conditional()
        || multiline_inside_collection(cx, node)
        || statement_non_eligible_node(cx, node)
}

fn multiline_inside_collection(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if node.is_modifier_form() {
        return false;
    }
    let Some(collection) = find_containing_collection(node) else {
        return false;
    };
    collection
        .child_nodes()
        .any(|child| sibling_if_shares_line(cx, child, node))
}

fn sibling_if_shares_line(cx: &Investigation<'_, '_>, child: Node<'_>, node: Node<'_>) -> bool {
    let Some(inner) = unwrap_begin(child) else {
        return false;
    };
    inner.is(NodeType::If) && !inner.is_ternary() && shares_line_with(cx, inner, node)
}

fn unwrap_begin(node: Node<'_>) -> Option<Node<'_>> {
    let node = if node.is(NodeType::Pair) {
        node.pair_value()?
    } else {
        node
    };
    if node.is(NodeType::Begin) {
        node.child(0)
    } else {
        Some(node)
    }
}

fn shares_line_with(cx: &Investigation<'_, '_>, inner: Node<'_>, node: Node<'_>) -> bool {
    let ps = cx.processed_source;
    let node_end = node.loc().end;
    let inner_end = inner.loc().end;
    node_end.is_some_and(|e| same_line(ps, inner.source_range(), e))
        || inner_end.is_some_and(|e| same_line(ps, e, node.source_range()))
}

fn find_containing_collection(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent();
    match parent {
        Some(p) if p.is(NodeType::Begin) => collection_from_ancestor(p.parent()),
        _ => collection_from_ancestor(parent),
    }
}

fn collection_from_ancestor(node: Option<Node<'_>>) -> Option<Node<'_>> {
    let node = node?;
    if node.is(NodeType::Array) || node.is_call() {
        return Some(node);
    }
    if node.is(NodeType::Pair) {
        return node.parent();
    }
    None
}

fn another_modifier_if_on_same_line(node: Node<'_>) -> bool {
    let Some(collection) = find_containing_collection(node) else {
        return false;
    };
    let line = node.first_line();
    collection.descendants().any(|sibling| {
        sibling.is(NodeType::If)
            && sibling != node
            && sibling.is_modifier_form()
            && sibling.first_line() == line
    })
}

fn non_simple_if_unless(node: Node<'_>) -> bool {
    node.is_ternary() || node.is_elsif() || node.has_else()
}

fn another_statement_on_same_line(node: Node<'_>) -> bool {
    let line_no = node.last_line();
    // Traverse the AST upwards until we find a `begin` node; we want to look
    // at the following child and see if it is on the same line as this `if`.
    let mut current = Some(node);
    let mut index = None;
    while let Some(n) = current {
        if n.is(NodeType::Begin) {
            break;
        }
        index = n.sibling_index();
        current = n.parent();
    }
    let (Some(begin), Some(index)) = (current, index) else {
        return false;
    };
    match begin.children().get(index + 1) {
        Some(Child::Node(id)) => begin.ast().node(*id).first_line() == line_no,
        _ => false,
    }
}

fn to_normal_form(node: Node<'_>, indentation: &str) -> String {
    format!(
        "{} {}\n{indentation}  {}\n{indentation}end",
        node.keyword(),
        node.condition().unwrap().source(),
        node.body().unwrap().source()
    )
}

fn to_normal_form_with_heredoc(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    indentation: &str,
    (heredoc_body, heredoc_end): (Range, Range),
) -> String {
    let body = cx.source(heredoc_body);
    let end = cx.source(heredoc_end);
    format!(
        "{} {}\n{indentation}  {}\n{indentation}  {}\n{indentation}  {}\n{indentation}end",
        node.keyword(),
        node.condition().unwrap().source(),
        node.body().unwrap().source(),
        body.strip_suffix('\n').unwrap_or(body),
        end.strip_suffix('\n').unwrap_or(end),
    )
}

fn to_modifier_form_with_move_comment(
    node: Node<'_>,
    indentation: &str,
    comment: &Comment<'_>,
) -> String {
    format!(
        "{}\n{indentation}{} {} {}",
        comment.text,
        node.body().unwrap().source(),
        node.keyword(),
        node.condition().unwrap().source()
    )
}

fn extract_heredoc_from(last_argument: Node<'_>) -> (Range, Range) {
    (
        last_argument.loc().heredoc_body.unwrap(),
        last_argument.loc().heredoc_end.unwrap(),
    )
}

fn remove_heredoc(cx: &Investigation<'_, '_>, corrector: &mut Corrector, heredoc: (Range, Range)) {
    for range in [heredoc.0, heredoc.1] {
        corrector.remove(range_by_whole_lines(cx.processed_source, range, true));
    }
}

fn comment_on_node_line<'s>(cx: &Investigation<'_, 's>, node: Node<'_>) -> Option<Comment<'s>> {
    cx.processed_source.comment_at_line(node.first_line()).copied()
}

fn remove_comment(cx: &Investigation<'_, '_>, corrector: &mut Corrector, comment: &Comment<'_>) {
    corrector.remove(range_with_surrounding_space(
        cx.processed_source.buffer_source(),
        comment.range,
        SurroundingSpace::side(Side::Left),
    ));
}
