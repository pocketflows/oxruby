//! Port of `lib/rubocop/cop/lint/useless_assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::variable_force::{Assignment, LOOP_TYPES, Scope, Variable, VariableTable};
use crate::cop::{Cop, Corrector, Investigation};
use crate::name_similarity;
use crate::processed_source::Range;

#[derive(Default)]
pub struct UselessAssignment {
    ignored: IgnoredNodes,
}

impl Cop for UselessAssignment {
    fn name(&self) -> &'static str {
        "Lint/UselessAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
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
            self.check_for_unused_assignments(cx, variable, variable_table);
        }
    }
}

impl UselessAssignment {
    fn check_for_unused_assignments<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        variable: &Variable<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        if variable.should_be_unused() {
            return;
        }
        for assignment in variable.assignments.iter().rev() {
            self.check_for_unused_assignment(cx, variable, assignment, variable_table);
        }
    }

    fn check_for_unused_assignment<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        variable: &Variable<'a>,
        assignment: &Assignment<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        let assignment_node = assignment.node;

        if self.ignored_assignment(variable, assignment_node, assignment) {
            return;
        }

        let message = message_for_useless_assignment(assignment, variable, variable_table);
        let range = offense_range(assignment);

        let uncorrectable = uncorrectable_assignment(assignment_node);
        cx.add_offense_with_correction(range, &message, |corrector| {
            if uncorrectable {
                return;
            }
            autocorrect(corrector, assignment, variable);
        });

        if chained_assignment(assignment_node) {
            self.ignored.ignore(assignment_node);
        }
    }

    fn ignored_assignment(
        &self,
        variable: &Variable<'_>,
        assignment_node: Node<'_>,
        assignment: &Assignment<'_>,
    ) -> bool {
        assignment.is_used(variable)
            || self.ignored.part_of(assignment_node)
            || variable_in_loop_condition(assignment_node, variable)
    }
}

/// Autocorrect is skipped when removing a variable would cause a syntax
/// error (`x = 1, y = 2`), or where rewriting `x ||= 1`/`x &&= 1` to
/// `x = 1` would raise `NameError` because the variable is not declared
/// before the operator assignment.
fn uncorrectable_assignment(assignment_node: Node<'_>) -> bool {
    sequential_assignment(assignment_node)
        || assignment_node
            .parent()
            .is_some_and(|p| p.is(NodeType::OrAsgn) || p.is(NodeType::AndAsgn))
}

fn message_for_useless_assignment(
    assignment: &Assignment<'_>,
    variable: &Variable<'_>,
    variable_table: &VariableTable<'_>,
) -> String {
    format!(
        "Useless assignment to variable - `{}`.{}",
        variable.name,
        message_specification(assignment, variable, variable_table).unwrap_or_default()
    )
}

fn offense_range(assignment: &Assignment<'_>) -> Range {
    if assignment.is_regexp_named_capture() {
        assignment
            .node
            .child(0)
            .map(|n| n.source_range())
            .unwrap_or_default()
    } else {
        assignment.node.loc().name.unwrap_or_default()
    }
}

/// `assignment?` as rubocop-ast answers it for any node
/// (`MethodDispatchNode` aliases it to `assignment_method?`).
fn node_assignment(node: Node<'_>) -> bool {
    match node.ty() {
        NodeType::Send | NodeType::Csend | NodeType::Indexasgn => node.is_assignment_method(),
        NodeType::Super
        | NodeType::Zsuper
        | NodeType::Yield
        | NodeType::Defined
        | NodeType::Index
        | NodeType::Lambda => false,
        _ => node.is_assignment(),
    }
}

fn sequential_assignment(node: Node<'_>) -> bool {
    if node.is(NodeType::Lvasgn)
        && node.expression().is_some_and(|e| e.is(NodeType::Array))
        && node.descendants().any(node_assignment)
    {
        return true;
    }
    match node.parent() {
        Some(parent) => sequential_assignment(parent),
        None => false,
    }
}

fn chained_assignment(node: Node<'_>) -> bool {
    if node.is(NodeType::Lvasgn) && node.expression().is_some_and(|e| e.is(NodeType::Send)) {
        return true;
    }
    // `respond_to?(:expression)`: of the assignment types, only `lvasgn`.
    node.is(NodeType::Lvasgn) && node.expression().is_some_and(|e| e.is(NodeType::Lvasgn))
}

fn message_specification(
    assignment: &Assignment<'_>,
    variable: &Variable<'_>,
    variable_table: &VariableTable<'_>,
) -> Option<String> {
    if assignment.is_multiple_assignment() {
        Some(multiple_assignment_message(variable.name))
    } else if assignment.is_operator_assignment() {
        operator_assignment_message(variable_table.scope(variable.scope), assignment)
    } else {
        similar_name_message(variable, variable_table)
    }
}

fn multiple_assignment_message(variable_name: &str) -> String {
    format!(
        " Use `_` or `_{variable_name}` as a variable name to indicate that it won't be used."
    )
}

fn operator_assignment_message(scope: &Scope<'_>, assignment: &Assignment<'_>) -> Option<String> {
    let return_value_node = return_value_node_of_scope(scope);
    if assignment.meta_assignment_node() != return_value_node {
        return None;
    }
    let operator = assignment.operator();
    Some(format!(
        " Use `{}` instead of `{operator}`.",
        operator.strip_suffix('=').unwrap_or(operator)
    ))
}

fn similar_name_message(variable: &Variable<'_>, variable_table: &VariableTable<'_>) -> Option<String> {
    let variable_like_names =
        collect_variable_like_names(variable_table.scope(variable.scope), variable_table);
    let similar_name = name_similarity::find_similar_name(variable.name, &variable_like_names)?;
    Some(format!(" Did you mean `{similar_name}`?"))
}

// TODO: More precise handling (rescue, ensure, nested begin, etc.)
fn return_value_node_of_scope<'a>(scope: &Scope<'a>) -> Option<Node<'a>> {
    let body_node = scope.body_node()?;
    if body_node.is(NodeType::Begin) {
        body_node.child_nodes().next_back()
    } else {
        Some(body_node)
    }
}

fn collect_variable_like_names<'a>(
    scope: &Scope<'a>,
    variable_table: &VariableTable<'a>,
) -> Vec<&'a str> {
    let mut names: Vec<&'a str> = Vec::new();
    let mut push = |name: &'a str| {
        if !names.contains(&name) {
            names.push(name);
        }
    };
    for node in scope.each_node() {
        if variable_like_method_invocation(node)
            && let Some(name) = node.method_name()
        {
            push(name);
        }
    }
    for variable in variable_table.scope_variables_of(scope) {
        push(variable.name);
    }
    names
}

fn variable_like_method_invocation(node: Node<'_>) -> bool {
    if !node.is(NodeType::Send) {
        return false;
    }
    node.receiver().is_none() && !node.has_arguments()
}

fn autocorrect(corrector: &mut Corrector, assignment: &Assignment<'_>, variable: &Variable<'_>) {
    if assignment.is_exception_assignment() {
        remove_exception_assignment_part(corrector, assignment.node);
    } else if assignment.is_multiple_assignment()
        || assignment.is_rest_assignment()
        || assignment.is_for_assignment()
    {
        rename_variable_with_underscore(corrector, assignment.node);
    } else if assignment.is_operator_assignment() {
        remove_trailing_character_from_operator(corrector, assignment.node);
    } else if assignment.is_regexp_named_capture() {
        replace_named_capture_group_with_non_capturing_group(
            corrector,
            assignment.node,
            variable.name,
        );
    } else {
        remove_local_variable_assignment_part(corrector, assignment.node);
    }
}

fn remove_exception_assignment_part(corrector: &mut Corrector, node: Node<'_>) {
    let Some(parent) = node.parent() else {
        return;
    };
    let range = parent
        .child(0)
        .map(|n| n.source_range())
        .or(parent.loc().keyword);
    if let Some(range) = range {
        corrector.remove(range.end().join(node.source_range().end()));
    }
}

fn rename_variable_with_underscore(corrector: &mut Corrector, node: Node<'_>) {
    corrector.replace(node.source_range(), "_");
}

fn remove_trailing_character_from_operator(corrector: &mut Corrector, node: Node<'_>) {
    if let Some(operator) = node.parent().and_then(|p| p.loc().operator) {
        corrector.remove(operator.end().adjust(-1, 0));
    }
}

fn replace_named_capture_group_with_non_capturing_group(
    corrector: &mut Corrector,
    node: Node<'_>,
    variable_name: &str,
) {
    let Some(regexp) = node.child(0) else {
        return;
    };
    let source = regexp.source();
    let replaced = source.replacen(&format!("(?<{variable_name}>"), "(?:", 1);
    corrector.replace(regexp.source_range(), replaced);
}

fn remove_local_variable_assignment_part(corrector: &mut Corrector, node: Node<'_>) {
    let (Some(name), Some(expression)) = (node.loc().name, node.expression()) else {
        return;
    };
    corrector.remove(name.begin().join(expression.source_range().begin()));
}

fn variable_in_loop_condition(assignment_node: Node<'_>, variable: &Variable<'_>) -> bool {
    if assignment_node.ancestors().any(|a| a.is_any_def()) {
        return false;
    }

    let Some(loop_node) = assignment_node
        .ancestors()
        .find(|ancestor| LOOP_TYPES.contains(&ancestor.ty()))
    else {
        return false;
    };

    // `respond_to?(:condition)`: `for` has none.
    if loop_node.is(NodeType::For) {
        return false;
    }
    let Some(condition_node) = loop_node.condition() else {
        return false;
    };
    let variable_name = variable.name;

    if condition_node.is(NodeType::Lvar) && condition_node.child_sym(0) == Some(variable_name) {
        return true;
    }

    condition_node
        .descendants()
        .any(|lvar_node| lvar_node.is(NodeType::Lvar) && lvar_node.child_sym(0) == Some(variable_name))
}
