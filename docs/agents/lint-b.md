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

- `cop/variable_force/` (claimed, landed).
- `crates/oxcop/src/name_similarity.rs` (claim `name_similarity`, landed):
  port of `lib/rubocop/name_similarity.rb` with Ruby 3.3's
  `DidYouMean::SpellChecker`, `JaroWinkler` and `Levenshtein`. Ruby's
  `sort_by!` is unstable; ties sort stably here.
- `cop/mixin/unused_argument.rs` (claim `unused_argument`) and
  `cop/correctors/unused_arg_corrector.rs` (claim `unused_arg_corrector`):
  committed on `lint-b`, **not landed yet** (shared files need the full gate,
  which is on hold while main's compare is red). The cop calls
  `unused_argument::check_argument(cx, variable, message_fn)` after its own
  checks.
- `cop/mixin/symbol_help.rs` (claim `symbol_help`): written, waiting with
  Lint/SymbolConversion.

## Cops done (upstream spec examples, all passing)

Landed:
- Lint/UselessAssignment 157 (1 pending upstream)

Landing in the cop-only batch (lint-b-land):
- Lint/RaiseException 17, RandOne 16, RedundantDirGlobSort 17,
  RedundantRequireStatement 12, RedundantSplatExpansion 64,
  RedundantStringCoercion 18, RedundantTypeConversion 613,
  RedundantWithIndex 19, RedundantWithObject 15, RefinementImportMethods 5,
  RegexpAsCondition 8, RequireParentheses 17, RequireRangeParentheses 10,
  RequireRelativeSelfPath 11, RescueException 11, RescueType 76,
  ReturnInVoidContext 19, SafeNavigationWithEmpty 7, SelfAssignment 69,
  SendWithMixinArgument 18, SharedMutableDefault 8, StructNewOverride 10,
  SuperArgumentMismatch 26 (25 pending: project index),
  SuppressedExceptionInNumberConversion 27, SuppressedException 24,
  ToEnumArguments 33, ToJSON 5, TopLevelReturnWithArgument 12,
  TrailingCommaInAttributeDeclaration 5, TripleQuotes 9,
  UnexpectedBlockArity 22, UnifiedInteger 9, UnmodifiedReduceAccumulator 94,
  UnreachableLoop 28, UnreachablePatternBranch 23,
  UnusedPrivateMethod 26 (25 pending: project index), UriEscapeUnescape 9,
  UriRegexp 10, UselessAccessModifier 198, UselessConstantScoping 28,
  UselessDefaultValueArgument 25, UselessDefined 7,
  UselessMethodDefinition 17, UselessNumericOperation 20, UselessOr 127,
  UselessRescue 10, UselessRuby2Keywords 25, UselessSetterCall 21,
  UselessTimes 32, Void 273

Committed on `lint-b`, waiting for the full gate (they need the
UnusedArgument mixin/corrector):
- Lint/UnusedBlockArgument 30, UnusedMethodArgument 46, ShadowedArgument 57,
  ShadowingOuterLocalVariable 33, UnderscorePrefixedVariableName 19

SuperArgumentMismatch and UnusedPrivateMethod need the `rubydex` project
index, which oxcop doesn't have; like RuboCop without one they never fire.

## Cops left

- Lint/SymbolConversion (written, needs the SymbolHelp mixin landing).
- Lint/ShadowedException (needs a table of Ruby exception classes; dump in
  progress).
- Lint/SafeNavigationChain, SafeNavigationConsistency (need style-d's
  NilMethods mixin to land), RedundantSafeNavigation (also needs
  `Lint::Utils::NilReceiverChecker`).
- Lint/RedundantRegexpQuantifiers, UnescapedBracketInRegexp: need a port of
  the `regexp_parser` gem (`node.parsed_tree`). Nobody has claimed one.
- Lint/UselessElseWithoutRescue (maximum target 2.5; every example is
  `unsupported_on: :prism`).
- Lint/ScriptPermission, Lint/UnreachableCode: see below.

## Principled failures

- **Lint/UnreachableCode** (7 of 267 fail): the `retry` examples put `retry`
  outside a `rescue`. Prism rejects that ("Invalid retry without rescue"), so
  the file has a syntax error and no cop runs. The specs run with the default
  TargetRubyVersion (2.7), where RuboCop parses with the `parser` gem, which
  accepts it. Proposed fix (shared, `processed_source.rs`): below target 3.3,
  drop Prism's "Invalid retry without rescue" error, as the `parser` gem
  would. Not landed; the cop is held back on `lint-b-wip`.
- **Lint/ScriptPermission**: the cop skips `--stdin` input, and the spec shim
  always sends `--stdin`; the spec also flips `@options[:autocorrect]` on the
  Ruby cop object. It needs shim support for inspecting the real file path
  (runtime's `spec/oxcop/`).
