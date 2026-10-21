# lint-a: Lint cops A–P

Scope: every `lib/rubocop/cop/lint/[a-p]*.rb` except AssignmentInCondition,
EnsureReturn, Syntax (done before) and MissingCopEnableDirective (`runtime`).

## Done (whole upstream spec passes; example counts)

BigDecimalNew 3, BinaryOperatorWithIdenticalOperands 23, BooleanSymbol 10,
DuplicateCaseCondition 9, DuplicateElsifCondition 5,
EachWithObjectArgument 7, EmptyEnsure 2, EmptyExpression 12, EmptyFile 4
(2 drive the Commissioner directly and never reach oxcop, like Lint/Syntax),
FlipFlop 2, FloatOutOfRange 5, HashCompareByIdentity 4,
IdentityComparison 12, Loop 4, MultipleComparison 20,
NumberedParameterAssignment 4, OrAssignmentToConstant 10.

## Shared changes

- `cop/base.rs`: `Investigation::add_global_offense` (with
  `offense::Location::no_location`, RuboCop's `Offense::NO_LOCATION`).
- `build.rs`: cop files named after Rust keywords (`loop.rs`) get an `r#`
  module name.

## Left

Everything else in A–P; see below for blockers.

## Blocked / deferred

- Tokens (`processed_source.tokens` is a stub): MisplacedMagicComment
  (`sorted_tokens`).
- Regexp parser (no `regexp_parser` port yet): DuplicateRegexpCharacterClassElement,
  MixedCaseRange, MixedRegexpCaptureTypes, OutOfRangeRegexpRef.
- ElseLayout needs the `alignment` mixin (claimed by another agent).

## Mixins owned

None yet.
