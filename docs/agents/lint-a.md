# lint-a: Lint cops A–P

Scope: every `lib/rubocop/cop/lint/[a-p]*.rb` except AssignmentInCondition,
EnsureReturn, Syntax (done before) and MissingCopEnableDirective (`runtime`).

## Done (whole upstream spec passes; example counts)

AmbiguousBlockAssociation 52, AmbiguousOperator 17,
AmbiguousRegexpLiteral 15, ArrayLiteralInRegexp 32, BigDecimalNew 3,
BinaryOperatorWithIdenticalOperands 23, BooleanSymbol 10,
CircularArgumentReference 13, ConstantDefinitionInBlock 27,
ConstantReassignment 68 (5 pending: project index),
ConstantResolution 21 (3 pending: project index),
ConstantOverwrittenInRescue 8, DataDefineOverride 8, Debugger 97,
DeprecatedClassMethods 31, DeprecatedConstants 16,
DeprecatedOpenSSLConstant 24,
DisjunctiveAssignmentInConstructor 7, DuplicateBranch 131,
DuplicateCaseCondition 9, DuplicateElsifCondition 5,
DuplicateMatchPattern 19, DuplicateRequire 10, DuplicateRescueException 6,
DuplicateSetElement 36, EachWithObjectArgument 7, EmptyBlock 17,
EmptyClass 9, EmptyConditionalBody 42, EmptyEnsure 2, EmptyExpression 12,
EmptyInterpolation 12,
EmptyFile 4 (2 drive the Commissioner directly and never reach oxcop, like
Lint/Syntax), EmptyInPattern 13, EmptyWhen 16, ErbNewArguments 10,
FlipFlop 2, FloatComparison 18, FloatOutOfRange 5,
HashCompareByIdentity 4, HashNewWithKeywordArgumentsAsDefault 10,
HeredocMethodCallPosition 11, IdentityComparison 12,
ImplicitStringConcatenation 12, IncompatibleIoSelectWithFiberScheduler 20,
IneffectiveAccessModifier 8, InheritException 18 (5 pending: they need the
`rubydex` project index), InterpolationCheck 17,
ItWithoutArgumentsInBlock 19, LambdaWithoutLiteralBlock 7,
LiteralAsCondition 261, LiteralAssignmentInCondition 35,
LiteralInInterpolation 396, Loop 4,
MissingSuper 31 (9 pending: project index), MultipleComparison 20, NestedMethodDefinition 38,
NextWithoutAccumulator 18, NoReturnInBeginEndBlocks 72,
NonAtomicFileOperation 43, NonDeterministicRequireOrder 14, NonLocalExitFromIterator 15,
NumberConversion 38, NumberedParameterAssignment 4,
NumericOperationWithConstantResult 15, OrAssignmentToConstant 10,
ParenthesesAsGroupedExpression 35.

All of them match RuboCop on ../code (`--only` with every landed Lint cop
but NumberConversion, whose ../code diff is rubocop-rails's plugin default
config, not yet loaded by oxcop).

## Waiting on a full-gate landing (branch `lint-a-wip`)

Shared changes can't land while compare_with_rubocop is red (plugin default
configs). Ready on `lint-a-wip`: AmbiguousOperatorPrecedence 13,
AmbiguousRange 54, DuplicateHashKey 33, with the `rational_literal` and
`duplication` mixins and the runner change below. Next in line, drafted:
the `percent_literal` mixin (NestedPercentLiteral, PercentStringArray,
PercentSymbolArray) and `cop/utils/format_string.rs`
(FormatParameterMismatch).

## Prism warnings

AmbiguousOperator and AmbiguousRegexpLiteral read Prism's warnings
(`processed_source.parse_result().warnings()`) and recognize the ones
`Prism::Translation::Parser` maps to `:ambiguous_prefix`/`:ambiguous_regexp`
by their message text: the Rust Prism API has no warning type.

## Shared changes

- `cop/base.rs`: `Investigation::add_global_offense` (with
  `offense::Location::no_location`, RuboCop's `Offense::NO_LOCATION`).
- `build.rs`: cop files named after Rust keywords (`loop.rs`) get an `r#`
  module name.
- (on `lint-a-wip`) `runner.rs`: with `--raw-config` (the spec shim) offenses are not
  `uniq`'d. RuboCop's runner merges offenses with the same line, column and
  message, but its specs look at the cop's offenses before that
  (AmbiguousOperatorPrecedence expects several offenses starting at 1:1).

## Left

ArgumentMismatch, AmbiguousAssignment, CopDirectiveSyntax,
DeprecatedReference,
DuplicateMagicComment, DuplicateMethods,
DuplicateRegexpCharacterClassElement, ElseLayout,
FormatParameterMismatch, MisplacedMagicComment,
MixedCaseRange, MixedRegexpCaptureTypes, NameTypo, NestedPercentLiteral,
OrderedMagicComments, OutOfRangeRegexpRef, PercentStringArray,
PercentSymbolArray.

## Blocked / deferred

- Index-only (`rubydex`): ArgumentMismatch, DeprecatedReference, NameTypo
  do nothing without a project index (every check starts with
  `return unless project_index`); all but one example of each spec is
  pending upstream without rubydex. A port would be an empty cop whose spec
  passes vacuously, so they are not ported. Decision for the user/coordinator.

- Tokens (`processed_source.tokens` is a stub): MisplacedMagicComment
  (`sorted_tokens`); DuplicateMagicComment and OrderedMagicComments
  (`leading_comment_lines` finds the first non-comment token).
- Regexp parser (no `regexp_parser` port yet): DuplicateRegexpCharacterClassElement,
  MixedCaseRange, MixedRegexpCaptureTypes, OutOfRangeRegexpRef.
- DuplicateMethods: its spec runs two files through one cop instance
  (`expect_no_offenses(src, 'first.rb')` then `expect_offense(src,
  'second.rb')`, "defined at both first.rb:2 and second.rb:2"). The shim
  starts a fresh cop per investigation; it needs cop instances kept for the
  whole example (runtime).
- CopDirectiveSyntax: needs every RuboCop cop name (the registry, not just
  the config's) for "Unknown cop name", plus NameSimilarity (lint-b).
- ElseLayout needs the `alignment` mixin (claimed by another agent).
- `check_assignment` (claimed by layout-a): AmbiguousAssignment.

## Mixins owned

- `rational_literal`, `duplication` (landed with AmbiguousRange and
  DuplicateHashKey), `percent_literal` (claimed).

## Note for node.rs owners

`Node::is_assignment` is type-based only; rubocop-ast aliases
`assignment?` to `setter_method?` on send nodes. My cops that need it
check `is_setter_method()` too (AmbiguousBlockAssociation,
IncompatibleIoSelectWithFiberScheduler).

## Note for the coordinator

`script/land` with a shared change runs the full gate (several minutes);
cop-only landings by other agents move main meanwhile, so a full-gate
landing can lose all 8 compare-and-swap attempts (it happened once here).
I now land cop-only batches separately and keep shared changes small.
