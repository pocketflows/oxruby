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
- Style/RedundantCondition 102
- Style/RedundantConstantBase 11 (3 pending: need the `rubydex` project index, as upstream without it)
- Style/RedundantCurrentDirectoryInPath 13
- Style/RedundantEach 36
- Style/RedundantDoubleSplatHashBraces 30
- Style/RedundantFetchBlock 15
- Style/RedundantException 32
- Style/RedundantFileExtensionInRequire 5
- Style/RedundantFilterChain 45
- Style/RedundantFreeze 47
- Style/RedundantHeredocDelimiterQuotes 17
- Style/RedundantInterpolationUnfreeze 16
- Style/RedundantInitialize 23
- Style/RedundantPercentQ 27
- Style/RedundantMinMaxBy 33
- Style/RedundantRegexpArgument 50
- Style/RedundantRegexpConstructor 11
- Style/RedundantSelfAssignmentBranch 22
- Style/RedundantSelfAssignment 14
- Style/RedundantSelf 65
- Style/RedundantSort 50
- Style/RedundantReturn 39
- Style/RedundantSortBy 8
- Style/RedundantStructKeywordInit 12
- Style/RegexpLiteral 65
- Style/RequireOrder 24
- Style/ReturnNil 5
- Style/ReturnNilInPredicateMethodDefinition 39
- Style/ReverseFind 14

## Pending / WIP branches (local, not on main)

`style-c-pending`: green, waiting only on a landing window or on another
agent's mixin. Land in this order once main's full gate is green:

1. `Port Style/RescueStandardError; add RescueNode mixin` (37) — shared
   mixin, needs the full gate; doesn't depend on the TEMP commits, so
   cherry-pick it onto `style-c` and land it alone.
2. `TEMP: copy of style-d's ReparsedEquivalence` + Style/RedundantParentheses
   (337). When style-d lands `mixin/reparsed_equivalence.rs`, drop the TEMP
   commit (`git rebase --onto main <TEMP-sha> style-c-pending`).
3. `TEMP: indentation helpers of layout-b's Alignment and its MatchRange` +
   Style/RedundantConditional (11), Style/ParallelAssignment (91),
   Style/RedundantStringEscape (328). Drop TEMP once layout-b lands
   `alignment.rs` and `match_range.rs` (their API is the same).

`style-c-wip`: ported but blocked by a spec that can't pass yet (see below):
Style/NumberedParametersLimit, Style/NumericLiterals, Style/RedundantArgument,
Style/RescueModifier.

## Left

- Next (MinBodyLength: style-b), OneLineConditional (OnNormalIfUnless and
  IfThenCorrector: style-b; Alignment: layout-b).
- PercentLiteralDelimiters, PercentQLiterals, RedundantCapitalW,
  RedundantInterpolation (PercentLiteral: lint-a).
- QuotedSymbols (SymbolHelp: lint-b).
- RedundantFormat (`utils/format_string`: lint-a).
- RedundantLineContinuation (needs tokens, MatchRange, ReparsedEquivalence).
- RedundantRegexpEscape, RedundantRegexpCharacterClass: need a port of the
  `regexp_parser` gem (`RegexpNode#parsed_tree`). Claimed by style-c as
  `regexp_parser`; also unblocks several Lint cops.

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

## Mixins owned

- `cop/mixin/integer_node.rs` (IntegerNode)

## Shared-file edits

- `cop/mixin/allowed_pattern.rs`: `allowed_pattern_regexes` and
  `config_regex` accept Ruby regexps in config (`!ruby/regexp /src/flags`, as
  the spec shim dumps `AllowedPatterns: [/foo/]`); `matches_allowed_pattern`
  uses them. `allowed_patterns` (strings only) is unchanged.
