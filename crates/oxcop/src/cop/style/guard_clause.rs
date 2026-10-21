//! Port of `lib/rubocop/cop/style/guard_clause.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::line_length_help::max_line_length;
use crate::cop::mixin::min_body_length::min_body_length;
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Guard {
    If,
    Else,
}

#[derive(Default)]
pub struct GuardClause;

impl Cop for GuardClause {
    fn name(&self) -> &'static str {
        "Style/GuardClause"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/MissingElse"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Def,
            NodeType::Defs,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::If,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Def | NodeType::Defs => on_def(cx, node),
            NodeType::If => on_if(cx, node),
            _ => {
                if node.is_method("define_method") || node.is_method("define_singleton_method") {
                    on_def(cx, node);
                }
            }
        }
    }
}

fn on_def(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    check_ending_body(cx, node.body());
}

/// `guard_clause?`: the matched node (the `rhs` of an `and`/`or`).
fn guard_clause(node: Node<'_>) -> Option<Node<'_>> {
    if !node.is_guard_clause() {
        return None;
    }
    if node.is_operator_keyword() {
        node.rhs()
    } else {
        Some(node)
    }
}

fn on_if(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if accepted_form(node, false) {
        return;
    }
    let (guard_clause, kw, mut guard) =
        if let Some(g) = node.if_branch().and_then(guard_clause) {
            (g, node.range_source(node.loc().keyword.unwrap()), Some(Guard::If))
        } else if let Some(g) = node.else_branch().and_then(guard_clause) {
            (g, node.inverse_keyword(), Some(Guard::Else))
        } else {
            return;
        };
    if and_or_guard_clause(guard_clause) {
        guard = None;
    }
    let source = guard_clause_source(guard_clause);
    register_offense(cx, node, source, kw, guard);
}

fn check_ending_body(cx: &mut Investigation<'_, '_>, body: Option<Node<'_>>) {
    let Some(body) = body else {
        return;
    };
    if body.is(NodeType::If) {
        check_ending_if(cx, body);
    } else if body.is(NodeType::Begin)
        && let Some(final_expression) = body.child_nodes().last()
        && final_expression.is(NodeType::If)
    {
        check_ending_if(cx, final_expression);
    }
}

fn check_ending_if(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if accepted_form(node, true) || !min_body_length(cx, node) {
        return;
    }
    if cx.cop_config_bool("AllowConsecutiveConditionals", false)
        && node
            .parent()
            .is_some_and(|parent| consecutive_conditionals(parent, node))
    {
        return;
    }
    register_offense(cx, node, "return", node.inverse_keyword(), None);
    check_ending_body(cx, node.if_branch());
}

fn consecutive_conditionals(parent: Node<'_>, node: Node<'_>) -> bool {
    let mut if_type = false;
    for child in parent.child_nodes() {
        if child == node {
            return if_type;
        }
        if_type = child.is(NodeType::If);
    }
    if_type
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    scope_exiting_keyword: &str,
    conditional_keyword: &str,
    guard: Option<Guard>,
) {
    let condition = node.condition().unwrap();
    let mut example = format!(
        "{scope_exiting_keyword} {conditional_keyword} {}",
        condition.source()
    );
    let mut replacement = None;
    if too_long_for_single_line(cx, node, &example) {
        if trivial(node) {
            return;
        }
        example = format!(
            "{conditional_keyword} {}; {scope_exiting_keyword}; end",
            condition.source()
        );
        replacement = Some(format!(
            "{conditional_keyword} {}\n  {scope_exiting_keyword}\nend",
            condition.source()
        ));
    }
    let message = format!(
        "Use a guard clause (`{example}`) instead of wrapping the code inside a conditional expression."
    );
    let replacement = replacement.unwrap_or_else(|| example.clone());
    let ps = cx.processed_source;
    cx.add_offense_with_correction(node.loc().keyword.unwrap(), &message, |corrector| {
        if node.has_else() && guard.is_none() {
            return;
        }
        autocorrect(ps, corrector, node, condition, &replacement, guard);
    });
}

fn autocorrect(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    condition: Node<'_>,
    replacement: &str,
    guard: Option<Guard>,
) {
    corrector.replace(
        node.loc().keyword.unwrap().join(condition.source_range()),
        replacement,
    );
    let if_branch = node.if_branch();
    let else_branch = node.else_branch();
    if node.has_then() {
        corrector.replace(node.loc().begin.unwrap(), "\n");
    }
    if let Some(if_heredoc) = find_heredoc_argument(if_branch) {
        autocorrect_heredoc_argument(ps, corrector, node, if_heredoc, else_branch, guard);
    } else if let Some(else_heredoc) = find_heredoc_argument(else_branch) {
        autocorrect_heredoc_argument(ps, corrector, node, else_heredoc, if_branch, guard);
    } else {
        corrector.remove(node.loc().end.unwrap());
        if !node.has_else() {
            return;
        }
        corrector.remove(node.loc().else_.unwrap());
        if let Some(range) = range_of_branch_to_remove(node, guard) {
            corrector.remove(range);
        }
    }
}

fn heredoc(argument: Node<'_>) -> bool {
    argument.is_any_str() && argument.is_heredoc()
}

fn find_heredoc_argument(node: Option<Node<'_>>) -> Option<Node<'_>> {
    let mut node = node?;
    while node.is(NodeType::Begin) {
        node = node.child_nodes().next()?;
    }
    if heredoc(node) {
        return Some(node);
    }
    if !node.is_call() {
        return None;
    }
    for argument in node.arguments().into_iter().rev() {
        if let Some(heredoc_argument) = find_heredoc_argument(Some(argument)) {
            return Some(heredoc_argument);
        }
    }
    find_heredoc_argument(node.receiver())
}

fn autocorrect_heredoc_argument(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    heredoc_node: Node<'_>,
    leave_branch: Option<Node<'_>>,
    guard: Option<Guard>,
) {
    remove_whole_lines(ps, corrector, node.loc().end.unwrap());
    if !node.has_else() {
        return;
    }
    if let Some(leave_branch) = leave_branch {
        remove_whole_lines(ps, corrector, leave_branch.source_range());
        corrector.insert_after(
            heredoc_node.loc().heredoc_end.unwrap(),
            format!("\n{}", leave_branch.source()),
        );
    }
    remove_whole_lines(ps, corrector, node.loc().else_.unwrap());
    if let Some(range) = range_of_branch_to_remove(node, guard) {
        remove_whole_lines(ps, corrector, range);
    }
}

fn range_of_branch_to_remove(node: Node<'_>, guard: Option<Guard>) -> Option<Range> {
    let branch = match guard? {
        Guard::If => node.if_branch(),
        Guard::Else => node.else_branch(),
    };
    branch.map(|b| b.source_range())
}

fn guard_clause_source(guard_clause: Node<'_>) -> &str {
    if and_or_guard_clause(guard_clause) {
        guard_clause.parent().unwrap().source()
    } else {
        guard_clause.source()
    }
}

fn and_or_guard_clause(guard_clause: Node<'_>) -> bool {
    guard_clause
        .parent()
        .is_some_and(|p| p.is_operator_keyword())
}

fn too_long_for_single_line(cx: &Investigation<'_, '_>, node: Node<'_>, example: &str) -> bool {
    max_line_length(cx).is_some_and(|max| node.column() + example.chars().count() > max)
}

fn accepted_form(node: Node<'_>, ending: bool) -> bool {
    accepted_if(node, ending)
        || node.condition().is_some_and(|c| c.is_multiline())
        || node.parent().is_some_and(|p| {
            if p.is_call() {
                p.is_setter_method()
            } else {
                p.is_assignment()
            }
        })
}

fn trivial(node: Node<'_>) -> bool {
    let Some(if_branch) = node.if_branch() else {
        return false;
    };
    node.branches().iter().filter(|b| b.is_some()).count() == 1
        && !if_branch.is(NodeType::If)
        && !if_branch.is(NodeType::Begin)
}

fn accepted_if(node: Node<'_>, ending: bool) -> bool {
    if node.is_modifier_form()
        || node.is_ternary()
        || node.is_elsif_conditional()
        || assigned_lvar_used_in_if_branch(node)
    {
        return true;
    }
    if ending {
        node.has_else()
    } else {
        !node.has_else() || node.is_elsif()
    }
}

fn assigned_lvar_used_in_if_branch(node: Node<'_>) -> bool {
    let Some(if_branch) = node.if_branch() else {
        return false;
    };
    let Some(condition) = node.condition() else {
        return false;
    };
    let assigned: Vec<&str> = condition
        .descendants()
        .filter(|d| d.is(NodeType::Lvasgn))
        .filter_map(|d| d.child_sym(0))
        .collect();
    if_branch
        .descendants()
        .filter(|d| d.is(NodeType::Lvar))
        .any(|d| assigned.contains(&d.source()))
}

fn remove_whole_lines(ps: &ProcessedSource<'_>, corrector: &mut Corrector, range: Range) {
    corrector.remove(range_by_whole_lines(ps, range, true));
}
