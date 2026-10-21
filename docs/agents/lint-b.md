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
- `crates/oxcop/src/regexp_parser/` (claim `regexp_parser`): a port of the
  regexp_parser gem 2.13.1 (scanner, lexer, parser, expression tree), and
  `crates/oxcop/src/ext/` (`regexp_node.rs`, `regexp_parser.rs`), RuboCop's
  `ext/regexp_node.rb` and `ext/regexp_parser.rb`. Use it like this:

  ```rust
  use crate::ext::regexp_node::{parsed_tree, each_capture};
  let Some(tree) = parsed_tree(node) else { return };  // node.parsed_tree
  for expr in tree.tree.each_expression(Tree::ROOT, false) {
      let exp = tree.get(expr);          // .ty, .token, .text, .quantifier, .expressions
      let ts = tree.tree.ts(expr);       // char offset in the regexp body
      let range = tree.expression(expr); // expr.expression (a byte Range)
      // tree.body(expr), tree.quantifier(expr), tree.set_begin/set_end(expr)
  }
  ```

  `Tree::traverse` gives `(Event, ExprId)` pairs; `cop::mixin::util::
  parse_regexp(text)` is `Util#parse_regexp`. `script/compare_regexp_parser
  DIR...` diffs it against the gem (0 mismatches over 12,867 regexps from
  rubocop, ../code and the installed gems, plus 40,000 fuzzed ones). Note:
  RuboCop always passes `options:`, and `options & NOENCODING` is 0, which
  Ruby treats as true, so `parsed_tree` scans in binary mode (`\xff` is a
  `hex` escape, never `utf8_hex`). lint-a (DuplicateRegexpCharacterClass-
  Element, MixedCaseRange, MixedRegexpCaptureTypes) and the style agents
  (RedundantRegexpEscape, RedundantRegexpCharacterClass) can use it once it
  lands.
- `cop/mixin/unused_argument.rs` (claim `unused_argument`),
  `cop/correctors/unused_arg_corrector.rs` (claim `unused_arg_corrector`),
  `cop/mixin/symbol_help.rs` (claim `symbol_help`).
- `cop/lint/utils/nil_receiver_checker.rs` (`Lint::Utils::NilReceiverChecker`),
  included with `#[path]` by `redundant_safe_navigation.rs`, its only user.

**Not landed yet** (everything on `lint-b` past main touches shared files and
needs the full gate, which the coordinator put on hold while main's compare
is red): the regexp_parser port, the mixins above, the `processed_source.rs`
change below, and the cops that need them.

## Cops done (upstream spec examples, all passing)

Landed on main:
- Lint/UselessAssignment 157 (1 pending upstream)
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
- Lint/ShadowedException 39 (Ruby's exception classes come from a table
  dumped from a Ruby 3.3 process running RuboCop: core, `Errno`, the stdlib
  RuboCop loads), UselessElseWithoutRescue 1

Committed on `lint-b`, waiting for the full gate:
- Lint/UnusedBlockArgument 30, UnusedMethodArgument 46, ShadowedArgument 57,
  ShadowingOuterLocalVariable 33, UnderscorePrefixedVariableName 19
- Lint/RedundantSafeNavigation 118, SymbolConversion 46,
  RedundantRegexpQuantifiers 22, UnescapedBracketInRegexp 51,
  UnreachableCode 267

SuperArgumentMismatch and UnusedPrivateMethod need the `rubydex` project
index, which oxcop doesn't have; like RuboCop without one they never fire.
UselessElseWithoutRescue only applies to Ruby <= 2.5 (`parser`'s
`:useless_else`); under Prism it never fires.

## Cops left

- Lint/SafeNavigationChain, SafeNavigationConsistency: need style-d's
  NilMethods mixin (`cop/mixin/nil_methods.rs`, on `style-d`, not on main).
- Lint/ScriptPermission: see below.

## Principled failures and decisions

- **Lint/UnreachableCode**: its `retry` examples put `retry` outside a
  `rescue`. Prism rejects that ("Invalid retry without rescue"); the specs
  run at the default TargetRubyVersion (2.7), where RuboCop parses with the
  `parser` gem, which accepts it. `ProcessedSource::new` now drops that one
  Prism error below Ruby 3.3 (shared file; lands with the full gate).
- **Lint/ScriptPermission**: the cop skips `--stdin` input, and the spec shim
  always sends `--stdin`; the spec also flips `@options[:autocorrect]` on the
  Ruby cop object. It needs shim support for inspecting the real file path
  (runtime's `spec/oxcop/`). Not ported.
