//! Port of `lib/rubocop/cop/lint/unused_method_argument.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::unused_argument;
use crate::cop::variable_force::{Scope, Variable, VariableTable};
use crate::cop::{Cop, Investigation};

static NOT_IMPLEMENTED: NodePattern = NodePattern::new(
    "{(send nil? :raise #allowed_exception_class? ...)
      (send nil? :fail ...)}",
);

#[derive(Default)]
pub struct UnusedMethodArgument;

impl Cop for UnusedMethodArgument {
    fn name(&self) -> &'static str {
        "Lint/UnusedMethodArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/ExplicitBlockArgument"]
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
    if !variable.is_method_argument() {
        return;
    }
    if variable.is_keyword_argument() && cx.cop_config_bool("AllowUnusedKeywordArguments", false) {
        return;
    }
    if ignored_method(cx, variable.scope_node.body()) {
        return;
    }
    if block_argument_with_yield(variable) {
        return;
    }

    unused_argument::check_argument(cx, variable, |variable| {
        message(variable, variable_table)
    });
}

fn block_argument_with_yield(variable: &Variable<'_>) -> bool {
    if !variable.declaration_node.is(NodeType::Blockarg) {
        return false;
    }
    let Some(method_body) = variable.scope_node.body() else {
        return false;
    };
    method_body.is(NodeType::Yield) || method_body.descendants().any(|d| d.is(NodeType::Yield))
}

fn ignored_method(cx: &Investigation<'_, '_>, body: Option<Node<'_>>) -> bool {
    (cx.cop_config_bool("IgnoreEmptyMethods", false) && body.is_none())
        || (cx.cop_config_bool("IgnoreNotImplementedMethods", false)
            && body.is_some_and(|body| not_implemented(cx, body)))
}

fn not_implemented(cx: &Investigation<'_, '_>, body: Node<'_>) -> bool {
    let allowed_class_names = cx.cop_config_list("NotImplementedExceptions");
    let call = |name: &str, val: &Val<'_>, _args: &[Val<'_>]| {
        name == "allowed_exception_class?"
            && val.node().is_some_and(|node| {
                node.is(NodeType::Const)
                    && node
                        .const_name()
                        .is_some_and(|n| allowed_class_names.contains(&n.as_str()))
            })
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    NOT_IMPLEMENTED.match_with(body, &ctx).is_some()
}

fn message(variable: &Variable<'_>, variable_table: &VariableTable<'_>) -> String {
    let mut message = format!("Unused method argument - `{}`.", variable.name);

    if !variable.is_keyword_argument() {
        message.push_str(&format!(
            " If it's necessary, use `_` or `_{}` as an argument name to indicate that it won't be used. If it's unnecessary, remove it.",
            variable.name
        ));
    }

    let scope = variable_table.scope(variable.scope);
    let none_referenced = !variable_table
        .scope_variables(variable.scope)
        .filter(|v| v.is_method_argument())
        .any(|v| v.is_referenced());

    if none_referenced {
        message.push_str(&format!(
            " You can also write as `{}(*)` if you want the method to accept any arguments but don't care about them.",
            scope.name().unwrap_or_default()
        ));
    }

    message
}
