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
- Style/RedundantRegexpConstructor 11
- Style/RedundantSelfAssignmentBranch 22
- Style/RedundantSelfAssignment 14
- Style/RedundantSelf 65
- Style/RedundantSort 50
- Style/RedundantReturn 39
- Style/RedundantSortBy 8
- Style/RedundantStructKeywordInit 12
- Style/RequireOrder 24
- Style/ReturnNil 5
- Style/ReturnNilInPredicateMethodDefinition 39
- Style/ReverseFind 14

## Left

Everything else in Style N–R (see `../rubocop/lib/rubocop/cop/style/[n-r]*.rb`).

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

## Mixins owned

- `cop/mixin/integer_node.rs` (IntegerNode)

## Shared-file edits

- `cop/mixin/allowed_pattern.rs`: `allowed_pattern_regexes` and
  `config_regex` accept Ruby regexps in config (`!ruby/regexp /src/flags`, as
  the spec shim dumps `AllowedPatterns: [/foo/]`); `matches_allowed_pattern`
  uses them. `allowed_patterns` (strings only) is unchanged.
