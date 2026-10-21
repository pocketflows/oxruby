# style-b: Style cops F–M

Branch `style-b`. Held ports (complete, but their specs can't fully pass
yet) are on `style-b-wip`.

## Done (whole upstream spec passes; example counts)

FetchEnvVar 39, FileEmpty 17, FileNull 15, FileOpen 21, FileRead 30,
FileTouch 4, FileWrite 50, FloatDivision 34, For 35, FormatString 48,
GlobalStdStream 6, GlobalVars 74, HashAsLastArrayItem 19, HashConversion 25,
HashEachMethods 62, HashExcept 110, HashFetchChain 34, HashLikeCase 8,
HashSlice 114, HashTransformKeys 40, HashTransformValues 40,
IdenticalConditionalBranches 49, IfInsideElse 23, IfUnlessModifier 144,
IfWithBooleanLiteralBranches 94, IfWithSemicolon 36, ImplicitRuntimeError 8,
InPatternThen 7, InlineComment 4, InverseMethods 110, IpAddresses 14,
ItAssignment 23, ItBlockParameter 34, KeywordArgumentsMerging 10,
KeywordParametersOrder 11, Lambda 41, LambdaCall 20,
MapCompactWithConditionalBlock 33, MapIntoArray 64, MapJoin 23, MapToHash 38,
MapToSet 32, MethodCallWithArgsParentheses 149,
MethodCallWithoutArgsParentheses 39, MethodCalledOnDoEndBlock 10,
MethodDefParentheses 53, MinMax 12, MinMaxComparison 19, MissingElse 84,
MissingRespondToMissing 17 (6 pending: need `rubydex`, as upstream without
it), MixinGrouping 18, MixinUsage 19, ModuleFunction 12,
ModuleMemberExistenceCheck 69, MultilineBlockChain 11, MultilineIfThen 14,
MultilineInPatternThen 13, MultilineMethodSignature 21,
MultilineTernaryOperator 15, MultilineWhenThen 13, MultipleComparison 36.

61 cops. 43 of them (the ones needing no new shared helper) are on main
(b6c30e6, landed from a cop-only branch while main's full gate was red). The
other 18 need the full gate because they come with shared changes: FileRead,
FileWrite, For, HashExcept, HashLikeCase, HashSlice, HashTransformKeys,
HashTransformValues, IfInsideElse, IfUnlessModifier, IfWithSemicolon,
KeywordArgumentsMerging, Lambda, MapIntoArray, MethodCallWithArgsParentheses,
MissingElse, MultilineIfThen, MultilineMethodSignature. Land `style-b` with
`script/land` once main is green.

## Left

- Waiting on `tokens` (`processed_source.tokens`): FrozenStringLiteralComment,
  LineEndConcatenation, MagicCommentFormat.
- Waiting on `layout-b`'s Alignment mixin reaching main (`offset`,
  `indentation`, `configured_indentation_width`): InfiniteLoop,
  MultilineIfModifier, MultilineMemoization.

## Blocked on the spec shim / runtime (ports done, on `style-b-wip`)

- **`config_to_allow_offenses`** (ConfigurableEnforcedStyle's
  `correct_style_detected`/`opposite_style_detected`/`style_detected`, which
  only feed `--auto-gen-config`): specs assert on `cop.config_to_allow_offenses`
  after `expect_offense`. Style/HashSyntax 163/172 (all 9 failures),
  Style/FormatStringToken 348/420 (all 72). Other agents' cops hit it too
  (StringLiterals, SymbolArray, WordArray, NumericLiterals, SpecialGlobalVars).
  Needs: the detection bookkeeping in `configurable_enforced_style.rs` /
  `Investigation`, the binary reporting it, and the shim setting it on the
  spec's cop. **Decision for the coordinator/runtime.** HashSyntax is enabled
  in `../code`, so it matters.
- **Cop errors**: Style/GuardClause 75/76 — "Invalid MinBodyLength fails with
  an error" expects `raise_error('MinBodyLength needs to be a positive
  integer!')`. The cop panics with that message; the shim raises
  `Binary::Error` with the whole stderr instead. Style/Next and
  Style/MagicCommentFormat specs have the same pattern. Needs the binary to
  report a cop error in a parsable way and the shim to re-raise it as a
  `RuntimeError` with the message.
- **Offense dedup**: Style/HashLookupMethod 19/21 — `a[:x][:y]` gives two
  offenses at the same line/column with the same message; `Runner` dedups
  them (RuboCop's runner does `offenses.uniq` too), but the spec helper calls
  the cop directly. Needs the spec path to skip the dedup.

## Mixins and correctors owned

- `cop/mixin/statement_modifier.rs`, `line_length_help.rs` (embeds
  `URI::RFC2396_PARSER`'s `X_ABS_URI` pattern from uri 0.13 for
  `uri_regexp`; `valid_uri?` is taken as true for its matches).
- `cop/mixin/hash_subset.rs`, `hash_transform_method.rs`,
  `min_branches_count.rs`, `min_body_length.rs` (Style/Next needs it),
  `on_normal_if_unless.rs`, `hash_shorthand_syntax.rs`.
- `cop/correctors/for_to_each_corrector.rs`, `each_to_for_corrector.rs`,
  `if_then_corrector.rs`, `lambda_literal_to_method_corrector.rs`.
- `cop/mixin/reparsed_equivalence.rs` is `style-d`'s file, copied unchanged
  from their commit b26661f (they own it).

## Shared-file edits (small, additive)

- `ast/node_pattern.rs`: hyphenated node types (`block-pass`) mean
  `block_pass`; `%0` is the matched node (rubocop-ast's `param0`).
- `target_ruby.rs`: `TargetRubyVersion` given as a string (`'3.0'`) is read
  with `to_f`, as RuboCop does.
- `cop/mixin/configurable_enforced_style.rs`: `style_for` strips a leading
  `:` (a symbol style value from a spec's dumped config), like `to_sym`.

## Notes

- `IgnoredNodes`: cops whose Ruby code only uses `ignored_node?` (identity)
  reset it in `on_new_investigation`; cops using `part_of_ignored_node?`
  keep it across the spec's correction loop, as Ruby does.
