//! Port of `lib/rubocop/cop/style/identical_conditional_branches.rb`.

use std::cell::Cell;

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

#[derive(Clone, Copy, PartialEq, Eq)]
enum InsertPosition {
    AfterCondition,
    BeforeCondition,
}

#[derive(Default)]
pub struct IdenticalConditionalBranches;

impl Cop for IdenticalConditionalBranches {
    fn name(&self) -> &'static str {
        "Style/IdenticalConditionalBranches"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::Case, NodeType::CaseMatch]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let branches = match node.ty() {
            NodeType::If => {
                if node.is_elsif() {
                    return;
                }
                let mut branches = vec![node.if_branch()];
                branches.extend(expand_elses(node.else_branch()));
                branches
            }
            _ => {
                if node.loc().else_.is_none() || node.else_branch().is_none() {
                    return;
                }
                let mut branches: Vec<Option<Node<'a>>> =
                    node.when_branches().iter().map(|b| b.body()).collect();
                branches.push(node.else_branch());
                branches
            }
        };
        check_branches(cx, node, &branches);
    }
}

fn check_branches<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, branches: &[Option<Node<'a>>]) {
    // Return if any branch is empty. An empty branch can be an `if` without
    // an `else` or a branch that contains only comments.
    let Some(branches) = branches.iter().copied().collect::<Option<Vec<Node<'a>>>>() else {
        return;
    };

    let tails: Vec<Option<Node<'a>>> = branches.iter().map(|b| tail(*b)).collect();
    if duplicated_expressions(node, &tails) {
        check_expressions(cx, node, &tails, InsertPosition::AfterCondition);
    }

    if last_child_of_parent(node) && branches.iter().any(|b| single_child_branch(*b)) {
        return;
    }

    let heads: Vec<Option<Node<'a>>> = branches.iter().map(|b| head(*b)).collect();
    if !duplicated_expressions(node, &heads) {
        return;
    }

    let condition_variable = assignable_condition_value(node);

    if let Some(head) = heads[0]
        && is_assignment(head)
    {
        // The `send` node is used instead of the `indexasgn` node, so `name`
        // cannot be used.
        let assigned_value = if head.is(NodeType::Send) {
            head.receiver().map(|r| r.source().to_owned())
        } else {
            // `head.node_parts[0].to_s`: a node's `to_s` (its s-expression)
            // never equals a source.
            match head.children().first() {
                Some(Child::Sym(s)) => Some(s.clone()),
                Some(Child::Nil) => Some(String::new()),
                _ => None,
            }
        };
        if assigned_value.is_some() && condition_variable == assigned_value {
            return;
        }
    }

    check_expressions(cx, node, &heads, InsertPosition::BeforeCondition);
}

fn is_assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

fn duplicated_expressions(node: Node<'_>, expressions: &[Option<Node<'_>>]) -> bool {
    let Some(first) = expressions.first() else {
        return false;
    };
    let all_same = expressions.iter().all(|e| match (e, first) {
        (Some(a), Some(b)) => a.same_as(*b),
        (None, None) => true,
        _ => false,
    });
    if !all_same {
        return false;
    }
    let Some(unique_expression) = first.filter(|e| is_assignment(*e)) else {
        return true;
    };
    let Some(lhs) = unique_expression.child_nodes().next() else {
        return true;
    };
    match node.condition() {
        Some(condition) => !condition
            .child_nodes()
            .any(|n| n.is_variable() && n.source() == lhs.source()),
        None => true,
    }
}

fn assignable_condition_value(node: Node<'_>) -> Option<String> {
    let condition = node.condition()?;
    if condition.is_call() {
        Some(match condition.receiver() {
            Some(receiver) => receiver.source().to_owned(),
            None => condition.source().to_owned(),
        })
    } else if condition.is_variable() {
        Some(condition.source().to_owned())
    } else {
        None
    }
}

fn check_expressions<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    expressions: &[Option<Node<'a>>],
    insert_position: InsertPosition,
) {
    let Some(expressions) = expressions.iter().copied().collect::<Option<Vec<Node<'a>>>>() else {
        return;
    };
    let inserted_expression = Cell::new(false);
    for expression in expressions {
        let message = format!("Move `{}` out of the conditional.", expression.source());
        cx.add_offense_cx(expression.source_range(), &message, |corrector, cx| {
            if node.is(NodeType::If) && (node.is_ternary() || node.has_then()) {
                return;
            }
            let range = range_by_whole_lines(cx.processed_source, expression.source_range(), true);
            corrector.remove(range);
            if inserted_expression.get() {
                return;
            }
            if node.parent().is_some_and(is_assignment) {
                correct_assignment(cx, corrector, node, expression, insert_position);
            } else {
                correct_no_assignment(corrector, node, expression, insert_position);
            }
            inserted_expression.set(true);
        });
    }
}

fn correct_assignment(
    cx: &Investigation<'_, '_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    expression: Node<'_>,
    insert_position: InsertPosition,
) {
    let parent = node.parent().unwrap();
    let indentation = indentation_of(parent);
    if insert_position == InsertPosition::AfterCondition {
        let assignment = Range::new(parent.source_range().begin_pos, node.source_range().begin_pos);
        corrector.remove(assignment);
        corrector.insert_after(
            node.source_range(),
            format!("\n{indentation}{}{}", cx.source(assignment), expression.source()),
        );
    } else {
        corrector.insert_before(
            parent.source_range(),
            format!("{}\n{indentation}", expression.source()),
        );
    }
}

fn correct_no_assignment(
    corrector: &mut Corrector,
    node: Node<'_>,
    expression: Node<'_>,
    insert_position: InsertPosition,
) {
    let indentation = indentation_of(node);
    if insert_position == InsertPosition::AfterCondition {
        corrector.insert_after(
            node.source_range(),
            format!("\n{indentation}{}", expression.source()),
        );
    } else {
        corrector.insert_before(
            node.source_range(),
            format!("{}\n{indentation}", expression.source()),
        );
    }
}

/// The leading indentation of the line the conditional starts on.
fn indentation_of(node: Node<'_>) -> String {
    " ".repeat(node.column())
}

fn last_child_of_parent(node: Node<'_>) -> bool {
    match node.parent() {
        None => true,
        Some(parent) => parent.child_nodes().last().is_some_and(|l| l.same_as(node)),
    }
}

fn single_child_branch(branch_node: Node<'_>) -> bool {
    !branch_node.is(NodeType::Begin) || branch_node.children().len() == 1
}

/// `elsif` branches show up in the if node as nested `else` branches.
fn expand_elses(branch: Option<Node<'_>>) -> Vec<Option<Node<'_>>> {
    match branch {
        None => vec![None],
        Some(branch) if branch.is(NodeType::If) => {
            let mut result = vec![branch.child(1)];
            result.extend(expand_elses(branch.child(2)));
            result
        }
        Some(branch) => vec![Some(branch)],
    }
}

fn tail(node: Node<'_>) -> Option<Node<'_>> {
    if node.is(NodeType::Begin) {
        node.child_nodes().last()
    } else {
        Some(node)
    }
}

fn head(node: Node<'_>) -> Option<Node<'_>> {
    if node.is(NodeType::Begin) {
        node.child_nodes().next()
    } else {
        Some(node)
    }
}
