//! Port of `lib/rubocop/cop/style/redundant_self_assignment_branch.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove the self-assignment branch.";

#[derive(Default)]
pub struct RedundantSelfAssignmentBranch;

impl Cop for RedundantSelfAssignmentBranch {
    fn name(&self) -> &'static str {
        "Style/RedundantSelfAssignmentBranch"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Lvasgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(expression) = node.expression() else {
            return;
        };
        if !expression.is(NodeType::If) || (expression.is_ternary() && expression.has_else()) {
            return;
        }
        let if_branch = expression.if_branch();
        let else_branch = expression.else_branch();
        if multiple_statements(if_branch)
            || multiple_statements(else_branch)
            || else_branch.is_some_and(|e| e.is(NodeType::If) && e.is_elsif())
        {
            return;
        }
        let name = node.name().unwrap_or("");
        if self_assign(name, if_branch) {
            register_offense(cx, expression, if_branch.unwrap(), else_branch, "unless");
        } else if self_assign(name, else_branch) {
            register_offense(cx, expression, else_branch.unwrap(), if_branch, "if");
        }
    }
}

fn multiple_statements(branch: Option<Node<'_>>) -> bool {
    branch.is_some_and(|b| b.is(NodeType::Begin) && !b.children().is_empty())
}

fn self_assign(variable: &str, branch: Option<Node<'_>>) -> bool {
    branch.is_some_and(|b| b.source() == variable)
}

fn register_offense<'a>(
    cx: &mut Investigation<'a, '_>,
    if_node: Node<'a>,
    offense_branch: Node<'a>,
    opposite_branch: Option<Node<'a>>,
    keyword: &str,
) {
    cx.add_offense_with_correction(offense_branch.source_range(), MSG, |corrector| {
        let assignment_value = opposite_branch.map_or("nil", |b| b.source());
        let mut replacement = format!(
            "{assignment_value} {keyword} {}",
            if_node.condition().unwrap().source()
        );
        if let Some(opposite) = opposite_branch
            && opposite.is_any_str()
            && opposite.is_heredoc()
        {
            let heredoc_end = opposite.loc().heredoc_end.unwrap();
            replacement.push_str(
                opposite.range_source(heredoc_end.with_begin(opposite.source_range().end_pos)),
            );
        }
        corrector.replace(if_node.source_range(), replacement);
    });
}
