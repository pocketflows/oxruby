# layout-b: Layout cops L–Z

## Done (whole upstream spec passes; example counts)

- Layout/MultilineArrayBraceLayout 35
- Layout/MultilineHashBraceLayout 34
- Layout/MultilineMethodCallBraceLayout 44
- Layout/MultilineMethodDefinitionBraceLayout 26
- Layout/MultilineArrayLineBreaks 6
- Layout/MultilineHashKeyLineBreaks 10
- Layout/MultilineMethodArgumentLineBreaks 18
- Layout/MultilineMethodParameterLineBreaks 14
- Layout/ParameterAlignment 19
- Layout/SpaceAfterColon 12
- Layout/SpaceAfterMethodName 8
- Layout/SpaceAfterNot 6
- Layout/SpaceBeforeBrackets 28
- Layout/SpaceInsideRangeLiteral 7
- Layout/SpaceInLambdaLiteral 15
- Layout/SpaceAroundMethodCallOperator 51
- Layout/SingleLineBlockChain 9
- Layout/SpaceAroundBlockParameters 49
- Layout/SpaceAroundKeyword 110
- Layout/MultilineBlockLayout 30

## Shared helpers I own

- `cop/mixin/alignment.rs` (Alignment): `check_alignment` takes the cop's
  `message(node)` and `autocorrect(corrector, node)` as closures, and hands
  `column_delta` to `autocorrect` instead of keeping it in `@column_delta`.
  `each_bad_alignment` returns `(node, column_delta)` pairs.
- `cop/correctors/alignment_corrector.rs` (`correct`, `align_end`; a
  `Target::Node`/`Target::Range` stands for "node or range").
- `cop/mixin/multiline_literal_brace_layout.rs` and
  `cop/correctors/multiline_literal_brace_corrector.rs`: the cop passes
  `children(node)` and its four messages.
- `cop/mixin/multiline_element_line_breaks.rs`,
  `cop/correctors/empty_line_corrector.rs`.
- Additive: `Investigation::current_offense_ranges()` (for
  `@current_offenses` lookups) and `Investigation::max_line_length()`
  (`autocorrect_logic.rb`) in base.rs; `Config::cop_enabled_in_config`
  (`Config#cop_enabled?` alone, where `pending` is truthy) and
  `Config::for_enabled_cop` in config.rs.

## Left

Not token-based: LeadingEmptyLines (tokens), LineContinuationLeadingSpace,
LineEndStringConcatenationIndentation, LineLength, MultilineAssignmentLayout,
MultilineMethodCallIndentation, MultilineOperationIndentation,
RedundantLineBreak, RescueEnsureAlignment, SpaceBeforeFirstArg,
SpaceInsideArrayPercentLiteral, SpaceInsideBlockBraces,
SpaceInsidePercentLiteralDelimiters, TrailingEmptyLines.

Blocked on tokens: SpaceAfterComma, SpaceAfterSemicolon, SpaceBeforeComma,
SpaceBeforeSemicolon, SpaceBeforeComment, SpaceAroundEqualsInParameterDefault,
SpaceAroundOperators, SpaceInsideArrayLiteralBrackets,
SpaceInsideHashLiteralBraces, SpaceInsideParens, SpaceInsideReferenceBrackets,
SpaceInsideStringInterpolation, LineContinuationSpacing.

## Blocked on the spec shim (runtime)

- Layout/SpaceBeforeBlockBraces: ported (all offenses and corrections
  pass), but 9 of 18 examples assert `cop.config_to_allow_offenses`
  (the `--auto-gen-config` bookkeeping) and 1 expects the cop to raise
  `Unknown EnforcedStyleForEmptyBraces selected!`. The shim can't see
  either: it would need the binary to report `config_to_allow_offenses`
  per investigation and to surface a cop's exception message so
  `raise_error(msg)` matches. Same need in layout-a's DefEndAlignment and
  my MultilineOperationIndentation specs (11 spec files in all use
  `config_to_allow_offenses`). The port is parked on `layout-b-wip`.

## Principled failures

None yet.
