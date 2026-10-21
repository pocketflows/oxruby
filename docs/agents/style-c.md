# style-c: Style cops N–R

## Done (whole upstream spec passes; example counts)

- Style/NegatedIf 15
- Style/NegatedWhile 10
- Style/NestedTernaryOperator 7
- Style/NilComparison 10
- Style/Not 11
- Style/OpenStructUse 12
- Style/OptionHash 10
- Style/Proc 6
- Style/NegatedIfElseCondition 32
- Style/NestedFileDirname 4 (1 more is `unsupported_on: :prism`)
- Style/NestedParenthesizedCalls 12
- Style/NilLambda 31
- Style/NonNilCheck 22
- Style/NumberedParameters 4
- Style/NumericLiteralPrefix 15
- Style/NumericPredicate 37
- Style/ObjectThen 22
- Style/OneClassPerFile 21
- Style/OperatorMethodCall 202
- Style/OptionalArguments 13
- Style/OptionalBooleanParameter 8
- Style/PartitionInsteadOfDoubleSelect 37
- Style/PercentQLiterals 21
- Style/PerlBackrefs 14
- Style/PredicateWithKind 64
- Style/PreferredHashMethods 9
- Style/RaiseArgs 35
- Style/RandomWithOffset 29
- Style/ReduceToHash 23
- Style/RedundantArrayConstructor 13
- Style/RedundantArrayFlatten 10
- Style/RedundantAssignment 12
- Style/RedundantBegin 62
- Style/RedundantCapitalW 13
- Style/RedundantCondition 102
- Style/RedundantConstantBase 11 (3 pending: need the `rubydex` project index, as upstream without it)
- Style/RedundantCurrentDirectoryInPath 13
- Style/RedundantEach 36
- Style/RedundantDoubleSplatHashBraces 30
- Style/RedundantFetchBlock 15
- Style/RedundantException 32
- Style/RedundantFileExtensionInRequire 5
- Style/RedundantFormat 306
- Style/RedundantFilterChain 45
- Style/RedundantFreeze 47
- Style/RedundantHeredocDelimiterQuotes 17
- Style/RedundantInterpolation 29
- Style/RedundantInterpolationUnfreeze 16
- Style/RedundantInitialize 23
- Style/RedundantPercentQ 27
- Style/RedundantMinMaxBy 33
- Style/RedundantRegexpArgument 50
- Style/RedundantRegexpCharacterClass 49
- Style/RedundantRegexpConstructor 11
- Style/RedundantRegexpEscape 218
- Style/RedundantSelfAssignmentBranch 22
- Style/RedundantSelfAssignment 14
- Style/RedundantSelf 65
- Style/RedundantSort 50
- Style/RedundantReturn 39
- Style/RedundantSortBy 8
- Style/RedundantStructKeywordInit 12
- Style/RegexpLiteral 65
- Style/RequireOrder 24
- Style/RescueStandardError 37
- Style/ReturnNil 5
- Style/ReturnNilInPredicateMethodDefinition 39
- Style/ReverseFind 14

## Pending / WIP branches (local, not on main)

`style-c-pending` holds green cops waiting on a landing window (main's full
gate) or on another agent's mixin. Each mixin it needs but doesn't own is a
`TEMP:` commit copying the owner's file from their branch; when the owner
lands it, drop that commit (`git rebase --onto main <TEMP-sha>
style-c-pending`; `-i` isn't available). Commits, oldest first:

1. `TEMP` style-d's ReparsedEquivalence; Style/RedundantParentheses (337).
2. (landed: Style/RescueStandardError + RescueNode.)
3. `TEMP` layout-b's Alignment indentation helpers and MatchRange;
   Style/RedundantConditional (11), Style/ParallelAssignment (91),
   Style/RedundantStringEscape (328).
4. `TEMP` lint-a's PercentLiteral and Utils::FormatString (both now on main:
   drop those two files from the TEMP commit when rebasing), style-b's
   MinBodyLength, OnNormalIfUnless, IfThenCorrector; `TEMP` style-d's
   PreferredDelimiters; Style/RedundantCapitalW (13), Style/PercentQLiterals
   (21), Style/RedundantInterpolation (29) and Style/RedundantFormat (306)
   are landed (their mixins reached main); Style/OneLineConditional (104)
   is left.
5. `TEMP` lint-b's SymbolHelp; Style/QuotedSymbols (97).

All of these pass their whole spec on `style-c-pending` and clippy is clean.

`style-c-wip` holds cops that pass all but a few examples blocked on the
shim or runner (see below); some also need the pending branch's mixins:
Style/NumberedParametersLimit, Style/NumericLiterals, Style/RedundantArgument,
Style/RescueModifier, Style/Next, Style/PercentLiteralDelimiters.

## Left

- RedundantLineContinuation: needs `processed_source.tokens` (`tokens`),
  plus MatchRange and ReparsedEquivalence.

## Blocked / principled failures

- **auto-gen-config state through the shim** (owner: `runtime`, `spec/oxcop/`).
  Some specs assert `cop.config_to_allow_offenses` or
  `read_exclude_limit(cop, 'Max')` after `expect_offense`: Ruby cop-instance
  state for `--auto-gen-config` (`exclude_limit`, `self.max = n`,
  `config_to_allow_offenses = {...}`). The shim builds offenses from the
  binary's JSON and never sets that state, so it is always `{}`. Needs a
  channel binary → JSON → shim → Ruby cop object. Blocked cops (ported, on
  `style-c-wip`, everything else in their specs passes):
  - Style/NumberedParametersLimit (11/12; 1 auto-gen example)
  - Style/NumericLiterals (24/28; 4 auto-gen examples)
- **Invalid UTF-8 in a spec's cop config** (owner: `runtime`,
  `spec/oxcop/binary.rb`). `with_config` does `YAML.dump(config.to_h)`,
  which raises `ArgumentError: invalid byte sequence in UTF-8` for a config
  string like `"\x82"`. Dumping such strings as `!binary` (e.g. force
  `ASCII-8BIT` encoding on invalid strings before dumping) would fix it; the
  binary side already decodes `!binary` values for this cop.
  - Style/RedundantArgument (14/15), on `style-c-wip`.
- **The runner's final `offenses.uniq`** (owner: `runtime`, `runner.rs`).
  RuboCop's runner drops offenses equal in line, column, cop, message and
  severity; the cop-level specs don't. `blah rescue 1 rescue 2` gives two
  RescueModifier offenses at 1:1 (ranges 0..13 and 0..22) and the spec
  expects both. Skipping the dedup in spec mode (`--raw-config`) would fix it.
  - Style/RescueModifier (19/20), on `style-c-wip`.
- **Errors raised by a cop** (owner: `runtime`, `spec/oxcop/`). Some specs
  expect the cop to raise for bad config: `raise_error(ArgumentError)` or
  `raise_error('MinBodyLength needs to be a positive integer!')`. The binary
  can only exit with an error, which the shim turns into
  `Oxcop::Spec::Binary::Error`, so the class/message don't match. Needs the
  binary to report the Ruby exception class and message and the shim to
  re-raise it.
  - Style/Next (71/72), Style/PercentLiteralDelimiters (65/66), on
    `style-c-wip`.

## Mixins owned

- `cop/mixin/integer_node.rs` (IntegerNode)
- `cop/mixin/rescue_node.rs` (RescueNode): `modifier_locations(ps)` reads the
  `keyword_loc` of Prism's `RescueModifierNode`s (what `kRESCUE_MOD` tokens
  would give), so it works without `processed_source.tokens`.

## Shared-file edits

- `cop/mixin/allowed_pattern.rs`: `allowed_pattern_regexes` and
  `config_regex` accept Ruby regexps in config (`!ruby/regexp /src/flags`, as
  the spec shim dumps `AllowedPatterns: [/foo/]`); `matches_allowed_pattern`
  uses them. `allowed_patterns` (strings only) is unchanged.
