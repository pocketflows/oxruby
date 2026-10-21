# layout-b: Layout cops L–Z

## Done (whole upstream spec passes; example counts)

Landed on main (656c62a, cop-only landing):

- Layout/SpaceAfterColon 12, SpaceAfterMethodName 8, SpaceAfterNot 6,
  SpaceBeforeBrackets 28, SpaceInsideRangeLiteral 7, SpaceInLambdaLiteral 15,
  SpaceAroundMethodCallOperator 51, SingleLineBlockChain 9,
  SpaceAroundKeyword 110, TrailingEmptyLines 20,
  LineContinuationLeadingSpace 32

Green on `layout-b`, waiting for a full-gate landing (they add shared
files):

- Layout/MultilineArrayBraceLayout 35, MultilineHashBraceLayout 34,
  MultilineMethodCallBraceLayout 44, MultilineMethodDefinitionBraceLayout 26
- Layout/MultilineArrayLineBreaks 6, MultilineHashKeyLineBreaks 10,
  MultilineMethodArgumentLineBreaks 18, MultilineMethodParameterLineBreaks 14
- Layout/ParameterAlignment 19, SpaceAroundBlockParameters 49,
  MultilineBlockLayout 30, MultilineMethodCallIndentation 287,
  LineEndStringConcatenationIndentation 59, MultilineAssignmentLayout 34,
  MultilineOperationIndentation 99, RedundantLineBreak 114,
  SpaceInsidePercentLiteralDelimiters 262, SpaceInsideArrayPercentLiteral 129
- Token-based (need the `tokens` agent's token builder, cherry-picked onto
  `layout-b` as `Build processed_source.tokens ...`; it drops out of the
  branch when `tokens` lands it): SpaceAfterComma 9, SpaceAfterSemicolon 9,
  SpaceBeforeComma 6, SpaceBeforeSemicolon 9, SpaceBeforeComment 5,
  LeadingEmptyLines 10, LineContinuationSpacing 31, SpaceInsideParens 28,
  SpaceInsideReferenceBrackets 47, SpaceAroundEqualsInParameterDefault 11,
  SpaceInsideStringInterpolation 12, SpaceInsideArrayLiteralBrackets 99,
  SpaceInsideHashLiteralBraces 40, SpaceBeforeFirstArg 12,
  SpaceAroundOperators 111, RescueEnsureAlignment 99

## Shared helpers I own (claimed in `.coord/claims`)

- `cop/mixin/alignment.rs` (Alignment): `check_alignment` takes the cop's
  `message(node)` and `autocorrect(corrector, node)` as closures and hands
  `column_delta` to `autocorrect` instead of keeping it in `@column_delta`.
- `cop/correctors/alignment_corrector.rs` (`correct`, `align_end`; a
  `Target::Node`/`Target::Range` stands for "node or range").
- `cop/mixin/multiline_literal_brace_layout.rs`,
  `cop/correctors/multiline_literal_brace_corrector.rs`,
  `cop/mixin/multiline_element_line_breaks.rs`,
  `cop/correctors/empty_line_corrector.rs`.
- `cop/mixin/multiline_expression_indentation.rs` (a trait for the cop's
  hooks, plus free helpers), `cop/mixin/match_range.rs`.
- `ast/token.rs`: rubocop-ast's `Token` predicates (`is_comma`,
  `is_left_parens`, `is_space_after`, ...) as an `impl Token`.
- `cop/mixin/space_after_punctuation.rs`, `space_before_punctuation.rs`,
  `surrounding_space.rs` (trait for `autocorrect` + `@ignored_nodes`),
  `cop/correctors/space_corrector.rs`,
  `cop/mixin/preceding_following_alignment.rs` (a struct holding the Ruby
  mixin's memoized line sets; ExtraSpacing in layout-a's range will want
  `alignment_lines` from it).
- `cop/mixin/check_line_breakable.rs` (on `layout-b-wip` with LineLength).
- Copied verbatim from their owners (identical files, so rebases merge
  cleanly): `punctuation_corrector.rs` (style-d), `rational_literal.rs`
  (lint-a), `check_assignment.rs` and `end_keyword_alignment.rs` (layout-a),
  `check_single_line_suitability.rs` and `reparsed_equivalence.rs` (style-d),
  `line_length_help.rs` (style-b, on `layout-b-wip`).
- Additive in shared files: `Investigation::current_offense_ranges()` and
  `Investigation::max_line_length()` (base.rs); `Config::cop_enabled_in_config`
  (`Config#cop_enabled?` alone, `pending` is truthy) and
  `Config::for_enabled_cop` (config.rs).

## Left

Everything in L–Z is ported. Not landed: the parked cops below
(`layout-b-wip`), and everything under "Green on `layout-b`" until a
full-gate landing goes through.

## Blocked on the spec shim (runtime)

Ported, every offense and correction passes, but examples assert things the
shim can't see. Parked (the files are on `layout-b-wip`):

- Layout/SpaceBeforeBlockBraces: 9 of 18 examples assert
  `cop.config_to_allow_offenses` (the `--auto-gen-config` bookkeeping) and 1
  expects the cop to raise `Unknown EnforcedStyleForEmptyBraces selected!`.
- Layout/SpaceInsideBlockBraces: 42 of 43 pass; 1 expects the same raise.
- Layout/LineLength: 205 of 209 pass. 3 examples read
  `read_exclude_limit(cop)` (the `self.max = ...` exclude_limit, i.e.
  `config_to_allow_offenses`). 1 more (`LDAP::CONST` with `URISchemes:
  [LDAP]`) needs style-b's `LineLengthHelp#valid_uri` to reject what
  `URI.parse` rejects (it returns `true` for every match; Ruby's
  `URI::LDAP` raises NoMethodError on that opaque URI). Uses a local copy of
  `EndlessMethodRewriter#correct_to_multiline` until style-a lands that
  mixin.

What the shim would need: (1) the binary reports a cop's raised error
message for the investigation and the shim re-raises it as a RuntimeError
with that message (RuboCop's specs run the Commissioner with
`raise_error: true`); (2) the binary reports `config_to_allow_offenses` per
investigation and the shim sets it on `cop`.

## Notes for others

- runtime: the full gate's `compare_with_rubocop` oxcop pass took ~4.5
  CPU-minutes (debug build, ~400 cops) at one point, vs ~1 s with 18 cops.
  That makes the landing race long enough to lose several times in a row.

## Principled failures

None.
