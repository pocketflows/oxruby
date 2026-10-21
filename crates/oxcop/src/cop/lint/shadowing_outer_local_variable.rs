//! Port of `lib/rubocop/cop/lint/shadowing_outer_local_variable.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::variable_force::{Variable, VariableTable};
use crate::cop::{Cop, Investigation};

static RACTOR_BLOCK: NodePattern =
    NodePattern::new("(block (send (const nil? :Ractor) :new ...) ...)");

#[derive(Default)]
pub struct ShadowingOuterLocalVariable;

impl Cop for ShadowingOuterLocalVariable {
    fn name(&self) -> &'static str {
        "Lint/ShadowingOuterLocalVariable"
    }

    fn joins_variable_force(&self) -> bool {
        true
    }

    fn before_declaring_variable<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        variable: &Variable<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        if variable.should_be_unused() {
            return;
        }
        if RACTOR_BLOCK.is_match(variable.scope_node) {
            return;
        }

        let Some(outer_local_variable) = variable_table.find_variable(variable.name) else {
            return;
        };
        let outer_local_variable = variable_table.variable(outer_local_variable);
        if variable_used_in_declaration_of_outer(variable, outer_local_variable) {
            return;
        }
        if same_conditions_node_different_branch(variable, outer_local_variable) {
            return;
        }

        let message = format!("Shadowing outer local variable - `{}`.", variable.name);
        cx.add_offense(variable.declaration_node.source_range(), &message);
    }
}

fn variable_used_in_declaration_of_outer(
    variable: &Variable<'_>,
    outer_local_variable: &Variable<'_>,
) -> bool {
    variable
        .scope_node
        .ancestors()
        .any(|a| a.same_as(outer_local_variable.declaration_node))
}

fn same_conditions_node_different_branch(
    variable: &Variable<'_>,
    outer_local_variable: &Variable<'_>,
) -> bool {
    if different_case_in_branch(variable, outer_local_variable) {
        return true;
    }

    let Some(variable_node) = variable_node(variable) else {
        return false;
    };
    if !node_or_its_ascendant_conditional(variable_node) {
        return false;
    }

    let Some(outer_local_variable_node) =
        find_conditional_node_from_ascendant(outer_local_variable.declaration_node)
    else {
        return false;
    };
    if !outer_local_variable_node.is_conditional() {
        return false;
    }
    if variable_node.same_as(outer_local_variable_node) {
        return true;
    }

    outer_local_variable_node.is(NodeType::If)
        && outer_local_variable_node
            .else_branch()
            .is_some_and(|e| variable_node.same_as(e))
}

fn different_case_in_branch(variable: &Variable<'_>, outer_local_variable: &Variable<'_>) -> bool {
    let inner_branch = variable
        .scope_node
        .ancestors()
        .find(|a| a.is(NodeType::InPattern));
    let outer_branch = outer_local_variable
        .declaration_node
        .ancestors()
        .find(|a| a.is(NodeType::InPattern));

    let (Some(inner_branch), Some(outer_branch)) = (inner_branch, outer_branch) else {
        return false;
    };

    !inner_branch.same_as(outer_branch)
        && match (inner_branch.parent(), outer_branch.parent()) {
            (Some(a), Some(b)) => a.same_as(b),
            _ => false,
        }
}

fn variable_node<'a>(variable: &Variable<'a>) -> Option<Node<'a>> {
    let parent_node = variable.scope_node.parent()?;

    if parent_node.is(NodeType::When) {
        parent_node.parent()
    } else {
        Some(parent_node)
    }
}

fn find_conditional_node_from_ascendant(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if parent.is_conditional() {
        return Some(parent);
    }

    find_conditional_node_from_ascendant(parent)
}

fn node_or_its_ascendant_conditional(node: Node<'_>) -> bool {
    if node.is_conditional() {
        return true;
    }

    find_conditional_node_from_ascendant(node).is_some()
}
