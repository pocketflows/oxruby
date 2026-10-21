# style-b: Style cops F–M

Branch `style-b`. The one held port (complete, but its spec can't fully
pass yet) is on `style-b-wip`.

## Done (whole upstream spec passes; example counts), all on main

FetchEnvVar 39, FileEmpty 17, FileNull 15, FileOpen 21, FileRead 30,
FileTouch 4, FileWrite 50, FloatDivision 34, For 35, FormatString 48,
FrozenStringLiteralComment 103, GlobalStdStream 6, GlobalVars 74,
GuardClause 76, HashAsLastArrayItem 19, HashConversion 25,
HashEachMethods 62, HashExcept 110, HashFetchChain 34, HashLikeCase 8,
HashLookupMethod 21, HashSlice 114, HashSyntax 172, HashTransformKeys 40,
HashTransformValues 40, IdenticalConditionalBranches 49, IfInsideElse 23,
IfUnlessModifier 144, IfWithBooleanLiteralBranches 94, IfWithSemicolon 36,
ImplicitRuntimeError 8, InPatternThen 7, InfiniteLoop 28, InlineComment 4,
InverseMethods 110, IpAddresses 14, ItAssignment 23, ItBlockParameter 34,
KeywordArgumentsMerging 10, KeywordParametersOrder 11, Lambda 41,
LambdaCall 20, LineEndConcatenation 19, MagicCommentFormat 28,
MapCompactWithConditionalBlock 33, MapIntoArray 64, MapJoin 23, MapToHash 38,
MapToSet 32, MethodCallWithArgsParentheses 149,
MethodCallWithoutArgsParentheses 39, MethodCalledOnDoEndBlock 10,
MethodDefParentheses 53, MinMax 12, MinMaxComparison 19, MissingElse 84,
MissingRespondToMissing 17 (6 pending: need `rubydex`, as upstream without
it), MixinGrouping 18, MixinUsage 19, ModuleFunction 12,
ModuleMemberExistenceCheck 69, MultilineBlockChain 11, MultilineIfModifier 10,
MultilineIfThen 14, MultilineInPatternThen 13, MultilineMemoization 19,
MultilineMethodSignature 21, MultilineTernaryOperator 15, MultilineWhenThen 13,
MultipleComparison 36.

70 cops (plus IfUnlessModifierOfIfUnless, InvertibleUnlessCondition and
MutableConstant, which were already done). Last landing: main 625e0a7.

## Left: one cop, blocked on the spec shim (port on `style-b-wip`)

- **Style/FormatStringToken** 384/420. The 36 failures are
  `expect(cop.config_to_allow_offenses).to eq('Enabled' => false)` after an
  `expect_offense` plus `expect_correction`. In Ruby the same cop instance
  runs both, so `detected_style` accumulates (`template` in the offense run,
  then `annotated` in the corrected source), and the intersection becomes
  empty. The binary's auto-gen state is per investigation: it isn't carried
  across the correction loop's iterations (`--persist-cops`) or from the
  `expect_offense` call to the autocorrect call. Needs `runtime` to keep
  `AutoGenState` with the persisted cops and have the shim pass it between
  calls (or merge the reports the way `style_detected` would). The cop
  already records its detected styles.

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
- `style_for` strips a leading `:` but `supported_styles`/`alternative_styles`
  don't; a cop comparing `style(cx)` with `supported_styles(cx)` under a spec
  that passes symbols would mismatch. Nothing fails from it today.
- Every landing passed `script/land`'s gates, including
  `compare_with_rubocop ../code` (0 mismatched files).
