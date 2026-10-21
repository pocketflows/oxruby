//! Port of `lib/rubocop/cop/lint/unused_block_argument.rb`.

use crate::cop::mixin::unused_argument;
use crate::cop::variable_force::{Scope, Variable, VariableTable};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct UnusedBlockArgument;

impl Cop for UnusedBlockArgument {
    fn name(&self) -> &'static str {
        "Lint/UnusedBlockArgument"
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
            check_argument(cx, variable, variable_table);
        }
    }
}

fn check_argument<'a>(
    cx: &mut Investigation<'a, '_>,
    variable: &Variable<'a>,
    variable_table: &VariableTable<'a>,
) {
    if allowed_block(cx, variable)
        || allowed_keyword_argument(cx, variable)
        || used_block_local(variable)
    {
        return;
    }

    unused_argument::check_argument(cx, variable, |variable| {
        message(variable, variable_table)
    });
}

fn used_block_local(variable: &Variable<'_>) -> bool {
    variable.is_explicit_block_local_variable() && !variable.assignments.is_empty()
}

fn allowed_block(cx: &Investigation<'_, '_>, variable: &Variable<'_>) -> bool {
    !variable.is_block_argument() || (ignore_empty_blocks(cx) && empty_block(variable))
}

fn allowed_keyword_argument(cx: &Investigation<'_, '_>, variable: &Variable<'_>) -> bool {
    variable.is_keyword_argument() && allow_unused_keyword_arguments(cx)
}

fn message(variable: &Variable<'_>, variable_table: &VariableTable<'_>) -> String {
    let message = format!(
        "Unused {} - `{}`.",
        variable_type(variable),
        variable.name
    );

    if variable.is_explicit_block_local_variable() {
        message
    } else {
        augment_message(message, variable, variable_table)
    }
}

fn augment_message(
    message: String,
    variable: &Variable<'_>,
    variable_table: &VariableTable<'_>,
) -> String {
    let all_arguments: Vec<&Variable<'_>> = variable_table
        .scope_variables(variable.scope)
        .filter(|v| v.is_block_argument())
        .collect();

    let augmentation = if variable.scope_node.is_lambda() {
        message_for_lambda(variable, &all_arguments)
    } else {
        message_for_normal_block(variable, &all_arguments)
    };

    format!("{message} {augmentation}")
}

fn variable_type(variable: &Variable<'_>) -> &'static str {
    if variable.is_explicit_block_local_variable() {
        "block local variable"
    } else {
        "block argument"
    }
}

fn message_for_normal_block(variable: &Variable<'_>, all_arguments: &[&Variable<'_>]) -> String {
    if !all_arguments.iter().any(|v| v.is_referenced()) && !define_method_call(variable) {
        if all_arguments.len() > 1 {
            "You can omit all the arguments if you don't care about them.".to_owned()
        } else {
            "You can omit the argument if you don't care about it.".to_owned()
        }
    } else {
        message_for_underscore_prefix(variable)
    }
}

fn message_for_lambda(variable: &Variable<'_>, all_arguments: &[&Variable<'_>]) -> String {
    let message = message_for_underscore_prefix(variable);

    if !all_arguments.iter().any(|v| v.is_referenced()) {
        let proc_message = "Also consider using a proc without arguments instead of a lambda if you want it to accept any arguments but don't care about them.";
        format!("{message} {proc_message}")
    } else {
        message
    }
}

fn message_for_underscore_prefix(variable: &Variable<'_>) -> String {
    format!(
        "If it's necessary, use `_` or `_{}` as an argument name to indicate that it won't be used.",
        variable.name
    )
}

fn define_method_call(variable: &Variable<'_>) -> bool {
    variable
        .scope_node
        .child(0)
        .and_then(|call| call.child_sym(1))
        == Some("define_method")
}

fn empty_block(variable: &Variable<'_>) -> bool {
    variable.scope_node.child(2).is_none()
}

fn allow_unused_keyword_arguments(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowUnusedKeywordArguments", false)
}

fn ignore_empty_blocks(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("IgnoreEmptyBlocks", false)
}
