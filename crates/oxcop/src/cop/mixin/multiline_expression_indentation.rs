//! Port of `lib/rubocop/cop/mixin/multiline_expression_indentation.rb`.
//!
//! The cop-specific hooks (`relevant_node?`, `right_hand_side`,
//! `offending_range`, `message`, `autocorrect`) are the
//! [`MultilineExpressionIndentation`] trait; the shared helpers are free
//! functions.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::parentheses;
use crate::cop::{Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const KEYWORD_ANCESTOR_TYPES: &[NodeType] = &[
    NodeType::For,
    NodeType::If,
    NodeType::While,
    NodeType::Until,
    NodeType::Return,
];
const UNALIGNED_RHS_TYPES: &[NodeType] = &[
    NodeType::If,
    NodeType::While,
    NodeType::Until,
    NodeType::For,
    NodeType::Return,
    NodeType::Array,
    NodeType::Kwbegin,
];
pub const DEFAULT_MESSAGE_TAIL: &str = "an expression";
pub const ASSIGNMENT_MESSAGE_TAIL: &str = "an expression in an assignment";

/// The methods a cop including the mixin provides.
pub trait MultilineExpressionIndentation {
    fn relevant_node(&self, node: Node<'_>) -> bool;

    fn right_hand_side(&self, cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<Range>;

    fn offending_range<'a>(
        &mut self,
        cx: &Investigation<'a, '_>,
        node: Node<'a>,
        lhs: Node<'a>,
        rhs: Range,
        given_style: &str,
    ) -> Option<Range>;

    fn message<'a>(
        &self,
        cx: &Investigation<'a, '_>,
        node: Node<'a>,
        lhs: Node<'a>,
        rhs: Range,
    ) -> String;

    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, range: Range);
}

/// `on_send` / `on_csend`.
pub fn on_send<'a, C: MultilineExpressionIndentation>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
) {
    let Some(receiver) = node.receiver() else {
        return;
    };
    if node.is_method("[]") {
        return;
    }
    if !cop.relevant_node(node) {
        return;
    }
    let lhs = left_hand_side(receiver);
    let Some(rhs) = cop.right_hand_side(cx, node) else {
        return;
    };
    let style = style(cx);
    let range = cop.offending_range(cx, node, lhs, rhs, style);
    check(cop, cx, range, node, lhs, rhs);
}

/// `left_hand_side(lhs)`.
pub fn left_hand_side(mut lhs: Node<'_>) -> Node<'_> {
    while let Some(parent) = lhs.parent() {
        if parent.is_call() && parent.loc().dot.is_some() && !parent.is_assignment_method() {
            lhs = parent;
        } else {
            break;
        }
    }
    lhs
}

/// `correct_indentation(node)`.
pub fn correct_indentation(cx: &Investigation<'_, '_>, node: Node<'_>) -> usize {
    match kw_node_with_special_indentation(node) {
        Some(kw_node) if !postfix_conditional(kw_node) => {
            configured_indentation_width(cx)
                + cx.config
                    .for_cop("Layout/IndentationWidth")
                    .get("Width")
                    .and_then(serde_yaml_ng::Value::as_i64)
                    .unwrap_or(2) as usize
        }
        _ => configured_indentation_width(cx),
    }
}

/// `check(range, node, lhs, rhs)`.
pub fn check<'a, C: MultilineExpressionIndentation>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    range: Option<Range>,
    node: Node<'a>,
    lhs: Node<'a>,
    rhs: Range,
) {
    if let Some(range) = range {
        incorrect_style_detected(cop, cx, range, node, lhs, rhs);
    }
}

fn incorrect_style_detected<'a, C: MultilineExpressionIndentation>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    range: Range,
    node: Node<'a>,
    lhs: Node<'a>,
    rhs: Range,
) {
    let message = cop.message(cx, node, lhs, rhs);
    // The `opposite_style_detected` re-check with the alternative style
    // only feeds `--auto-gen-config`.
    cx.add_offense_cx(range, &message, |corrector, cx| {
        cop.autocorrect(corrector, cx, range);
    });
}

/// `indentation(node)`: the column of the first non-blank character on the
/// line where `node` starts.
pub fn indentation(ps: &ProcessedSource<'_>, node: Node<'_>) -> usize {
    let line = ps.source_line(ps.line_of(node.source_range().begin_pos));
    line.chars()
        .position(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
        .unwrap_or(0)
}

/// `operation_description(node, rhs)`.
pub fn operation_description(node: Node<'_>, rhs: Range) -> String {
    if let Some(ancestor) = kw_node_with_special_indentation(node) {
        return keyword_message_tail(ancestor);
    }
    if part_of_assignment_rhs(node, Some(rhs)).is_some() {
        return ASSIGNMENT_MESSAGE_TAIL.to_owned();
    }
    DEFAULT_MESSAGE_TAIL.to_owned()
}

fn keyword_message_tail(node: Node<'_>) -> String {
    let keyword = node.range_source(node.loc().keyword.unwrap());
    let kind = if keyword == "for" {
        "collection"
    } else {
        "condition"
    };
    let article = if keyword.starts_with(['i', 'u']) {
        "an"
    } else {
        "a"
    };
    format!("a {kind} in {article} `{keyword}` statement")
}

/// `kw_node_with_special_indentation(node)`.
pub fn kw_node_with_special_indentation(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors().find(|ancestor| {
        if !ancestor.is_any(KEYWORD_ANCESTOR_TYPES) {
            return false;
        }
        if ancestor.is(NodeType::If) && ancestor.is_ternary() {
            return false;
        }
        indented_keyword_expression(*ancestor)
            .is_some_and(|expr| within_node(node.source_range(), expr.source_range()))
    })
}

/// `indented_keyword_expression(node)`.
pub fn indented_keyword_expression(node: Node<'_>) -> Option<Node<'_>> {
    if node.is(NodeType::For) {
        node.child(1)
    } else {
        node.child(0)
    }
}

/// How `argument_in_method_call` matches calls.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ArgumentKind {
    WithOrWithoutParentheses,
    WithParentheses,
}

/// `argument_in_method_call(node, kind)`.
pub fn argument_in_method_call(node: Node<'_>, kind: ArgumentKind) -> Option<Node<'_>> {
    for a in node.ancestors() {
        if a.is(NodeType::Block) {
            return None;
        }
        if !a.is(NodeType::Send) {
            continue;
        }
        if a.is_setter_method() {
            continue;
        }
        if !(kind == ArgumentKind::WithOrWithoutParentheses
            || (kind == ArgumentKind::WithParentheses && parentheses(a)))
        {
            continue;
        }
        if a.arguments()
            .iter()
            .any(|arg| within_node(node.source_range(), arg.source_range()))
        {
            return Some(a);
        }
    }
    None
}

/// `part_of_assignment_rhs(node, candidate)`.
pub fn part_of_assignment_rhs(node: Node<'_>, candidate: Option<Range>) -> Option<Node<'_>> {
    for ancestor in node.ancestors() {
        if disqualified_rhs(candidate, ancestor) {
            return None;
        }
        if valid_rhs(candidate, ancestor) {
            return Some(ancestor);
        }
    }
    None
}

fn disqualified_rhs(candidate: Option<Range>, ancestor: Node<'_>) -> bool {
    ancestor.is_any(UNALIGNED_RHS_TYPES)
        || (ancestor.is(NodeType::Block) && part_of_block_body(candidate, ancestor))
}

fn valid_rhs(candidate: Option<Range>, ancestor: Node<'_>) -> bool {
    if ancestor.is(NodeType::Send) {
        valid_method_rhs_candidate(candidate, ancestor)
    } else if is_assignment(ancestor) {
        valid_rhs_candidate(candidate, assignment_rhs(ancestor))
    } else {
        false
    }
}

/// `assignment?` as each node class defines it (`MethodDispatchNode`
/// aliases it to `assignment_method?`).
fn is_assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_assignment_method()
    } else {
        node.is_assignment()
    }
}

fn valid_method_rhs_candidate(candidate: Option<Range>, node: Node<'_>) -> bool {
    node.is_setter_method() && valid_rhs_candidate(candidate, node.last_argument())
}

fn valid_rhs_candidate(candidate: Option<Range>, node: Option<Node<'_>>) -> bool {
    match candidate {
        None => true,
        Some(candidate) => node.is_some_and(|node| within_node(candidate, node.source_range())),
    }
}

fn part_of_block_body(candidate: Option<Range>, block_node: Node<'_>) -> bool {
    match (block_node.body(), candidate) {
        (Some(body), Some(candidate)) => within_node(candidate, body.source_range()),
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// `assignment_rhs(node)`.
pub fn assignment_rhs(node: Node<'_>) -> Option<Node<'_>> {
    match node.ty() {
        NodeType::Casgn | NodeType::OpAsgn => node.rhs(),
        NodeType::Send | NodeType::Csend => node.last_argument(),
        _ => node
            .children()
            .last()
            .and_then(|c| c.as_node())
            .map(|id| node.ast().node(id)),
    }
}

/// `not_for_this_cop?(node)`.
pub fn not_for_this_cop(node: Node<'_>) -> bool {
    node.ancestors()
        .any(|ancestor| grouped_expression(ancestor) || inside_arg_list_parentheses(node, ancestor))
}

/// `grouped_expression?(node)`.
pub fn grouped_expression(node: Node<'_>) -> bool {
    node.is(NodeType::Begin) && node.loc().begin.is_some()
}

/// `inside_arg_list_parentheses?(node, ancestor)`.
pub fn inside_arg_list_parentheses(node: Node<'_>, ancestor: Node<'_>) -> bool {
    if !(ancestor.is(NodeType::Send) && ancestor.is_parenthesized()) {
        return false;
    }
    node.source_range().begin_pos > ancestor.loc().begin.unwrap().begin_pos
        && node.source_range().end_pos < ancestor.loc().end.unwrap().end_pos
}

fn postfix_conditional(node: Node<'_>) -> bool {
    node.is(NodeType::If) && node.is_modifier_form()
}

/// `within_node?(inner, outer)` on ranges.
pub fn within_node(inner: Range, outer: Range) -> bool {
    inner.begin_pos >= outer.begin_pos && inner.end_pos <= outer.end_pos
}
