# lint-b notes

Scope: `cop/variable_force/`, then Lint cops R–Z (except
Lint/RedundantCopDisableDirective and Lint/RedundantCopEnableDirective,
which belong to `runtime`).

## VariableForce: landed

`crates/oxcop/src/cop/variable_force/` ports `variable_force.rb` and
`variable_force/*.rb` file by file. How to use it from a cop:

```rust
fn joins_variable_force(&self) -> bool { true }

fn after_leaving_scope<'a>(
    &mut self,
    cx: &mut Investigation<'a, '_>,
    scope: &Scope<'a>,
    variable_table: &VariableTable<'a>,
) {
    for variable in variable_table.scope_variables_of(scope) { ... }
}
```

- The six hooks (`before/after_entering_scope`, `before/after_leaving_scope`,
  `before/after_declaring_variable`) are default no-op methods on the `Cop`
  trait (`cop/base.rs`). Scope hooks get `&Scope`, variable hooks `&Variable`,
  both with the `&VariableTable`.
- Scopes and variables live in arenas in the table (`ScopeId`, `VariableId`).
  `variable.scope` is a `ScopeId` (`variable_table.scope(id)`), and
  `variable.scope_node` its node. `Assignment#used?` takes the variable:
  `assignment.is_used(variable)`.
- Branches are computed eagerly (they only depend on the AST).
- The Commissioner runs the force after `on_new_investigation` and before the
  walk, with the cops of that investigation that join it
  (`variable_force::investigate_cops`), like `Team.forces_for` per
  Commissioner. The edit to `cop/commissioner.rs` is one call.

## Shared helpers I own

- `cop/variable_force/` (claimed).
- `crates/oxcop/src/name_similarity.rs` (claim `name_similarity`): port of
  `lib/rubocop/name_similarity.rb` with Ruby 3.3's `DidYouMean::SpellChecker`,
  `JaroWinkler` and `Levenshtein`. Ruby's `sort_by!` is unstable; ties sort
  stably here.

## Cops done (upstream spec examples)

- Lint/UselessAssignment 157 (1 pending upstream)

## Cops left

Every other Lint cop R–Z.

## Principled failures

None yet.
