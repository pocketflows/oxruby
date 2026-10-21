# style-c: Style cops N–R

## Done (whole upstream spec passes; example counts)

- Style/NegatedIf 15
- Style/NegatedWhile 10
- Style/NestedTernaryOperator 7
- Style/Next 72
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
- Style/NumberedParametersLimit 12
- Style/NumericLiteralPrefix 15
- Style/NumericLiterals 28
- Style/NumericPredicate 37
- Style/ObjectThen 22
- Style/OneClassPerFile 21
- Style/OneLineConditional 104
- Style/OperatorMethodCall 202
- Style/OptionalArguments 13
- Style/OptionalBooleanParameter 8
- Style/ParallelAssignment 91
- Style/PartitionInsteadOfDoubleSelect 37
- Style/PercentLiteralDelimiters 66
- Style/PercentQLiterals 21
- Style/PerlBackrefs 14
- Style/PredicateWithKind 64
- Style/PreferredHashMethods 9
- Style/QuotedSymbols 97
- Style/RaiseArgs 35
- Style/RandomWithOffset 29
- Style/ReduceToHash 23
- Style/RedundantArrayConstructor 13
- Style/RedundantArrayFlatten 10
- Style/RedundantAssignment 12
- Style/RedundantBegin 62
- Style/RedundantCapitalW 13
- Style/RedundantCondition 102
- Style/RedundantConditional 11
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
- Style/RedundantLineContinuation 174
- Style/RedundantInitialize 23
- Style/RedundantParentheses 337
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
- Style/RescueModifier 20
- Style/RescueStandardError 37
- Style/ReturnNil 5
- Style/ReturnNilInPredicateMethodDefinition 39
- Style/ReverseFind 14

## Pending / WIP branches (local, not on main)

`style-c-pending` now only matters for Style/RedundantStringEscape (328,
green): it needs layout-b's `mixin/match_range.rs`, which isn't on main yet
(the branch carries a `TEMP` copy). Once it lands, cherry-pick
`Port Style/RedundantStringEscape` onto `style-c` and land it. Everything
else on that branch has landed.

`style-c-wip` now only matters for Style/RedundantArgument (14/15; see
below). The rest of what was parked there has landed.

## Left

Nothing outside `style-c-pending`/`style-c-wip` (below).
RedundantLineContinuation scans with the regexp directly instead of
layout-b's MatchRange (not on main yet); switch it to `each_match_range`
once that lands.

## Blocked / principled failures

- **Invalid UTF-8 in a spec's cop config** (owner: `runtime`,
  `spec/oxcop/binary.rb`). `with_config` does `YAML.dump(config.to_h)`,
  which raises `ArgumentError: invalid byte sequence in UTF-8` for a config
  string like `"\x82"`. Dumping such strings as `!binary` (e.g. force
  `ASCII-8BIT` encoding on invalid strings before dumping) would fix it; the
  binary side already decodes `!binary` values for this cop.
  - Style/RedundantArgument (14/15), on `style-c-wip`.

## Mixins owned

- `cop/mixin/integer_node.rs` (IntegerNode)
- `cop/mixin/rescue_node.rs` (RescueNode): `modifier_locations(ps)` reads the
  `keyword_loc` of Prism's `RescueModifierNode`s (what `kRESCUE_MOD` tokens
  would give), so it works without `processed_source.tokens`.

## Shared-file edits

- `cop/mixin/preferred_delimiters.rs` (style-d): added
  `ensure_valid_preferred_delimiters` (raises `ArgumentError` for unknown
  `PreferredDelimiters` keys), as upstream.

- `cop/mixin/allowed_pattern.rs`: `allowed_pattern_regexes` and
  `config_regex` accept Ruby regexps in config (`!ruby/regexp /src/flags`, as
  the spec shim dumps `AllowedPatterns: [/foo/]`); `matches_allowed_pattern`
  uses them. `allowed_patterns` (strings only) is unchanged.
