//! Port of `lib/rubocop/cop/lint/shadowed_argument.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::variable_force::{Reference, Scope, Variable, VariableTable};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct ShadowedArgument;

impl Cop for ShadowedArgument {
    fn name(&self) -> &'static str {
        "Lint/ShadowedArgument"
    }

    fn joins_variable_force(&self) -> bool {
        true
    }

    fn after_leaving_scope<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        scope: &Scope<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        for variable in variable_table.scope_variables_of(scope) {
            check_argument(cx, variable);
        }
    }
}

/// `def_node_search :uses_var?, '(lvar %)'`
fn uses_var(node: Node<'_>, name: &str) -> bool {
    node.each_node()
        .any(|n| n.is(NodeType::Lvar) && n.child_sym(0) == Some(name))
}

fn check_argument(cx: &mut Investigation<'_, '_>, argument: &Variable<'_>) {
    if !(argument.is_method_argument() || argument.is_block_argument()) {
        return;
    }
    if argument.is_explicit_block_local_variable() {
        return;
    }

    if let Some(node) = shadowing_assignment(cx, argument) {
        let message = format!(
            "Argument `{}` was shadowed by a local variable before it was used.",
            argument.name
        );
        cx.add_offense(node.source_range(), &message);
    }
}

fn shadowing_assignment<'a>(
    cx: &Investigation<'_, '_>,
    argument: &Variable<'a>,
) -> Option<Node<'a>> {
    if !argument.is_referenced() {
        return None;
    }

    let (node, location_known) = assignment_without_argument_usage(argument)?;
    let assignment_without_usage_pos = node.source_range().begin_pos;

    let references = argument_references(argument);

    let skip = references.iter().any(|reference| {
        if !reference.is_explicit() && ignore_implicit_references(cx) {
            return true;
        }
        reference_pos(reference.node) <= assignment_without_usage_pos
    });
    if skip {
        return None;
    }

    Some(if location_known {
        node
    } else {
        argument.declaration_node
    })
}

/// Yields (here: returns) the first assignment that doesn't use the
/// argument, and whether its location is known.
fn assignment_without_argument_usage<'a>(argument: &Variable<'a>) -> Option<(Node<'a>, bool)> {
    let mut location_known = true;
    for assignment in &argument.assignments {
        let assignment_node = assignment.meta_assignment_node().unwrap_or(assignment.node);

        if assignment_node.is_shorthand_asgn() {
            location_known = false;
            continue;
        }
        let Some(parent) = assignment_node.parent() else {
            location_known = false;
            continue;
        };

        let conditional_assignment = conditional_assignment(Some(parent), argument.scope_node);

        if !uses_var(assignment_node, argument.name) {
            if conditional_assignment {
                location_known = false;
                continue;
            }

            return Some((assignment.node, location_known));
        }
    }
    None
}

fn reference_pos(node: Node<'_>) -> usize {
    let node = match node.parent() {
        Some(parent) if parent.is(NodeType::Masgn) => parent,
        _ => node,
    };

    node.source_range().begin_pos
}

fn conditional_assignment(node: Option<Node<'_>>, stop_search_node: Node<'_>) -> bool {
    let Some(node) = node else {
        return false;
    };
    if node == stop_search_node {
        return false;
    }

    node.is_conditional()
        || node.is(NodeType::Block)
        || node.is(NodeType::Rescue)
        || conditional_assignment(node.parent(), stop_search_node)
}

fn argument_references<'v, 'a>(argument: &'v Variable<'a>) -> Vec<&'v Reference<'a>> {
    let assignment_references: Vec<Range> = argument
        .assignments
        .iter()
        .flat_map(|a| a.references.iter().map(|n| n.source_range()))
        .collect();

    argument
        .references
        .iter()
        .filter(|reference| {
            if !reference.is_explicit() {
                return true;
            }
            !assignment_references.contains(&reference.node.source_range())
        })
        .collect()
}

fn ignore_implicit_references(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IgnoreImplicitReferences", false)
}
