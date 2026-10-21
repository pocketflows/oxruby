# style-b: Style cops F–M

## Done (whole upstream spec passes; example counts)

FetchEnvVar 39, FileEmpty 17, FileNull 15, FileOpen 21, FileRead 30,
FileTouch 4, FileWrite 50, FloatDivision 34, For 35, FormatString 48,
GlobalStdStream 6, GlobalVars 74, HashConversion 25, HashExcept 110,
HashFetchChain 34, HashLikeCase 8, HashSlice 114, HashTransformKeys 40,
HashTransformValues 40, IdenticalConditionalBranches 49, IfInsideElse 23,
IfUnlessModifier 144, IfWithBooleanLiteralBranches 94, IfWithSemicolon 36,
ImplicitRuntimeError 8, InPatternThen 7, InlineComment 4, InverseMethods 110,
ItAssignment 23, KeywordArgumentsMerging 10, KeywordParametersOrder 11,
Lambda 41, LambdaCall 20.

## Left

Everything else in Style F–M (see `../rubocop/lib/rubocop/cop/style/[f-m]*.rb`).

## Blocked / principled failures

- **Style/HashLookupMethod** (19/21): two examples expect both offenses of
  `a[:x][:y]`, which start at the same line and column with the same message.
  `Runner` dedups offenses (`offenses.uniq`, as RuboCop's runner does), but
  RuboCop's spec helper calls the cop directly, without the runner's `uniq`.
  Needs the spec path (`--stdin`/server mode used by the shim) to skip the
  dedup — a `runtime` change. The port is ready (kept on `style-b-wip`).

## Mixins and correctors owned

- `cop/mixin/statement_modifier.rs`, `cop/mixin/line_length_help.rs`
  (embeds `URI::RFC2396_PARSER`'s `X_ABS_URI` pattern from uri 0.13 for
  `uri_regexp`; `valid_uri?` is taken as true for its matches).
- `cop/mixin/hash_subset.rs`, `hash_transform_method.rs`,
  `min_branches_count.rs`, `on_normal_if_unless.rs`.
- Claimed, not yet written: `min_body_length`, `hash_shorthand_syntax`.
- `cop/correctors/for_to_each_corrector.rs`, `each_to_for_corrector.rs`,
  `if_then_corrector.rs`, `lambda_literal_to_method_corrector.rs`.

## Shared-file edits

- `ast/node_pattern.rs`: hyphenated node types (`block-pass`) mean
  `block_pass`, as rubocop-ast's lexer does.
- `target_ruby.rs`: `TargetRubyVersion` given as a string (`'3.0'`) is read
  with `to_f`, as RuboCop does (several specs pass it as a string).
