//! Port of `lib/rubocop/cop/lint/duplicate_branch.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Duplicate branch body detected.";

#[derive(Default)]
pub struct DuplicateBranch;

impl Cop for DuplicateBranch {
    fn name(&self) -> &'static str {
        "Lint/DuplicateBranch"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Case,
            NodeType::CaseMatch,
            NodeType::Rescue,
            NodeType::If,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::If) && node.is_elsif() {
            return;
        }
        on_branching_statement(cx, node);
    }
}

fn on_branching_statement<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let branches = branches(node);
    let mut previous: Vec<Node<'a>> = Vec::new();
    for &branch in &branches {
        if !consider_branch(cx, &branches, branch) {
            continue;
        }
        if previous.iter().any(|p| p.same_as(branch)) {
            cx.add_offense(offense_range(branch), MSG);
        } else {
            previous.push(branch);
        }
    }
}

fn has_else_branch(node: Node<'_>) -> bool {
    matches!(
        node.ty(),
        NodeType::If | NodeType::Case | NodeType::CaseMatch | NodeType::Rescue
    )
}

fn offense_range(duplicate_branch: Node<'_>) -> Range {
    let parent = duplicate_branch.parent().unwrap();
    if has_else_branch(parent) && parent.else_branch() == Some(duplicate_branch) {
        if parent.is(NodeType::If) && parent.is_ternary() {
            duplicate_branch.source_range()
        } else {
            parent.loc().else_.unwrap()
        }
    } else {
        parent.source_range()
    }
}

fn branches(node: Node<'_>) -> Vec<Node<'_>> {
    node.branches().into_iter().flatten().collect()
}

fn consider_branch(cx: &Investigation<'_, '_>, branches: &[Node<'_>], branch: Node<'_>) -> bool {
    if ignore_literal_branches(cx) && literal_branch(cx, branch) {
        return false;
    }
    if ignore_constant_branches(cx) && const_branch(branch) {
        return false;
    }
    if ignore_duplicate_else_branches(cx) && duplicate_else_branch(branches, branch) {
        return false;
    }
    true
}

fn ignore_literal_branches(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IgnoreLiteralBranches", false)
}

fn ignore_constant_branches(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IgnoreConstantBranches", false)
}

fn ignore_duplicate_else_branches(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IgnoreDuplicateElseBranch", false)
}

fn literal_branch(cx: &Investigation<'_, '_>, branch: Node<'_>) -> bool {
    if !branch.is_literal() || branch.is(NodeType::Xstr) {
        return false;
    }
    if branch.is_basic_literal() {
        return true;
    }
    branch.descendants().all(|node| {
        node.is_basic_literal()
            || node.is(NodeType::Pair)
            || (node.is(NodeType::Const) && ignore_constant_branches(cx))
    })
}

fn const_branch(branch: Node<'_>) -> bool {
    branch.is(NodeType::Const)
}

fn duplicate_else_branch(branches: &[Node<'_>], branch: Node<'_>) -> bool {
    let Some(parent) = branch.parent() else {
        return false;
    };
    branches.len() > 2
        && branches.last() == Some(&branch)
        && has_else_branch(parent)
        && parent.has_else()
}
