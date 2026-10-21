//! Port of `lib/rubocop/cop/lint/underscore_prefixed_variable_name.rb`.

use crate::ast::NodeType;
use crate::cop::variable_force::{Scope, Variable, VariableTable};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not use prefix `_` for a variable that is used.";

#[derive(Default)]
pub struct UnderscorePrefixedVariableName;

impl Cop for UnderscorePrefixedVariableName {
    fn name(&self) -> &'static str {
        "Lint/UnderscorePrefixedVariableName"
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
            check_variable(cx, variable);
        }
    }
}

fn check_variable(cx: &mut Investigation<'_, '_>, variable: &Variable<'_>) {
    if !variable.should_be_unused() {
        return;
    }
    if !variable.references.iter().any(|r| r.is_explicit()) {
        return;
    }
    if allowed_keyword_block_argument(cx, variable) {
        return;
    }

    let node = variable.declaration_node;

    let location = if node.is(NodeType::MatchWithLvasgn) {
        node.child(0).map(|n| n.source_range())
    } else {
        node.loc().name
    };

    if let Some(location) = location {
        cx.add_offense(location, MSG);
    }
}

fn allowed_keyword_block_argument(cx: &Investigation<'_, '_>, variable: &Variable<'_>) -> bool {
    variable.is_block_argument()
        && variable.is_keyword_argument()
        && cx.cop_config_bool("AllowKeywordBlockArguments", false)
}
