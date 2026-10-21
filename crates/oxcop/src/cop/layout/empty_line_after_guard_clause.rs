//! Port of `lib/rubocop/cop/layout/empty_line_after_guard_clause.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Corrector, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

const MSG: &str = "Add empty line after guard clause.";
const END_OF_HEREDOC_LINE: usize = 1;

static GUARD_CLAUSE_BRANCH: NodePattern =
    NodePattern::new("{(send nil? {:raise :fail} ...) return break next}");

#[derive(Default)]
pub struct EmptyLineAfterGuardClause;

impl Cop for EmptyLineAfterGuardClause {
    fn name(&self) -> &'static str {
        "Layout/EmptyLineAfterGuardClause"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if correct_style(node) || multiple_statements_on_line(node) {
            return;
        }
        let ps = cx.processed_source;
        if node.is_modifier_form()
            && let Some(heredoc_node) = last_heredoc_argument(Some(node))
        {
            if next_line_empty_or_allowed_directive_comment(ps, heredoc_line(ps, node, heredoc_node))
            {
                return;
            }
            cx.add_offense_with_correction(
                heredoc_node.loc().heredoc_end.unwrap(),
                MSG,
                |corrector| autocorrect(ps, corrector, heredoc_node),
            );
        } else {
            if next_line_empty_or_allowed_directive_comment(ps, node.last_line()) {
                return;
            }
            cx.add_offense_with_correction(offense_location(node), MSG, |corrector| {
                autocorrect(ps, corrector, node)
            });
        }
    }
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let mut node_range = if heredoc(Some(node)) {
        range_by_whole_lines(ps, node.loc().heredoc_body.unwrap(), false)
    } else {
        range_by_whole_lines(ps, node.source_range(), false)
    };
    let next_line = ps.line_of(node_range.end_pos) + 1;
    if next_line_allowed_directive_comment(ps, next_line) {
        node_range = ps.comment_at_line(next_line).unwrap().range;
    }
    corrector.insert_after(node_range, "\n");
}

fn correct_style(node: Node<'_>) -> bool {
    !node.if_branch().is_some_and(|b| b.is_guard_clause())
        || next_line_rescue_or_ensure(node)
        || next_sibling_parent_empty_or_else(node)
        || next_sibling_empty_or_guard_clause(node)
}

fn contains_guard_clause(node: Node<'_>) -> bool {
    let Some(branch) = node.if_branch() else {
        return false;
    };
    branch.is_guard_clause() || GUARD_CLAUSE_BRANCH.is_match(branch)
}

fn next_line_empty_or_allowed_directive_comment(ps: &ProcessedSource<'_>, line: usize) -> bool {
    if next_line_empty(ps, line) {
        return true;
    }
    let next_line = line + 1;
    next_line_allowed_directive_comment(ps, next_line) && next_line_empty(ps, next_line)
}

/// `processed_source[line].blank?` (`line` is 1-based, so this is the
/// line after it).
fn next_line_empty(ps: &ProcessedSource<'_>, line: usize) -> bool {
    ps.lines()
        .get(line)
        .is_none_or(|l| l.chars().all(char::is_whitespace))
}

fn next_line_allowed_directive_comment(ps: &ProcessedSource<'_>, line: usize) -> bool {
    let Some(comment) = ps.comment_at_line(line) else {
        return false;
    };
    DirectiveComment::new(comment.text, comment.range, line as i64).enabled()
        || simplecov_directive_comment(comment.text)
}

fn next_line_rescue_or_ensure(node: Node<'_>) -> bool {
    match node.parent() {
        None => true,
        Some(parent) => parent.is(NodeType::Rescue) || parent.is(NodeType::Ensure),
    }
}

fn next_sibling_parent_empty_or_else(node: Node<'_>) -> bool {
    let Some(Child::Node(_)) = node.right_sibling_value() else {
        return true;
    };
    let next_sibling = node.right_sibling().unwrap();
    match next_sibling.parent() {
        Some(parent) => parent.is(NodeType::If) && parent.has_else(),
        None => false,
    }
}

fn next_sibling_empty_or_guard_clause(node: Node<'_>) -> bool {
    match node.right_sibling_value() {
        None | Some(Child::Nil) => true,
        Some(Child::Node(_)) => {
            let next_sibling = node.right_sibling().unwrap();
            next_sibling.is(NodeType::If) && contains_guard_clause(next_sibling)
        }
        Some(_) => false,
    }
}

/// `respond_to?(:arguments)` on rubocop-ast nodes.
fn responds_to_arguments(node: Node<'_>) -> bool {
    use NodeType::*;
    matches!(
        node.ty(),
        Send | Csend
            | Super
            | Zsuper
            | Yield
            | Defined
            | Def
            | Defs
            | Block
            | Numblock
            | Itblock
            | Return
            | Break
            | Next
            | Index
            | Indexasgn
            | Lambda
    )
}

fn last_heredoc_argument(node: Option<Node<'_>>) -> Option<Node<'_>> {
    let mut n = node.and_then(last_heredoc_argument_node);
    while let Some(inner) = n.filter(|n| n.is(NodeType::Begin)) {
        n = inner.child(0);
    }
    if heredoc(n) {
        return n;
    }
    let n = n.filter(|n| responds_to_arguments(*n))?;
    for argument in n.arguments() {
        if let Some(node) = last_heredoc_argument(Some(argument)) {
            return Some(node);
        }
    }
    last_heredoc_argument(n.receiver())
}

fn last_heredoc_argument_node(node: Node<'_>) -> Option<Node<'_>> {
    if !node.is(NodeType::If) {
        return Some(node);
    }
    let if_branch = node.if_branch()?;
    if if_branch.is(NodeType::And) {
        if_branch.child(0)
    } else if node.condition().is_some_and(use_heredoc_in_condition) {
        node.condition()
    } else {
        match if_branch.children().last() {
            Some(Child::Node(id)) => Some(if_branch.ast().node(*id)),
            _ => None,
        }
    }
}

fn heredoc_line(ps: &ProcessedSource<'_>, node: Node<'_>, heredoc_node: Node<'_>) -> usize {
    let heredoc_body = heredoc_node.loc().heredoc_body.unwrap();
    let num_of_heredoc_lines = ps.line_of(heredoc_body.end_pos) - ps.line_of(heredoc_body.begin_pos);
    node.last_line() + num_of_heredoc_lines + END_OF_HEREDOC_LINE
}

fn heredoc(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| n.is_any_str() && n.is_heredoc())
}

fn use_heredoc_in_condition(condition: Node<'_>) -> bool {
    condition.descendants().any(|d| heredoc(Some(d)))
}

fn offense_location(node: Node<'_>) -> Range {
    node.loc().end.unwrap_or(node.source_range())
}

fn multiple_statements_on_line(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is(NodeType::Begin)
        && node
            .right_sibling()
            .is_some_and(|sibling| node.first_line() == sibling.first_line())
}

fn simplecov_directive_comment(text: &str) -> bool {
    regex!(r"\A#\s*(?::nocov:|simplecov\s*:\s*(?:disable|enable)\b)")
        .find(text)
        .is_some()
}
