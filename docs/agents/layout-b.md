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
- `Investigation::current_offense_ranges()` (base.rs, additive) for
  `@current_offenses` lookups.

## Left

Not token-based: LeadingEmptyLines (tokens), LineContinuationLeadingSpace,
LineEndStringConcatenationIndentation, LineLength, MultilineAssignmentLayout,
MultilineBlockLayout, MultilineMethodCallIndentation,
MultilineOperationIndentation, RedundantLineBreak, RescueEnsureAlignment,
SingleLineBlockChain, SpaceAfterColon, SpaceAfterMethodName, SpaceAfterNot,
SpaceAroundBlockParameters, SpaceAroundKeyword, SpaceAroundMethodCallOperator,
SpaceBeforeBlockBraces, SpaceBeforeBrackets, SpaceBeforeFirstArg,
SpaceInLambdaLiteral, SpaceInsideArrayPercentLiteral, SpaceInsideBlockBraces,
SpaceInsidePercentLiteralDelimiters, SpaceInsideRangeLiteral, TrailingEmptyLines.

Blocked on tokens: SpaceAfterComma, SpaceAfterSemicolon, SpaceBeforeComma,
SpaceBeforeSemicolon, SpaceBeforeComment, SpaceAroundEqualsInParameterDefault,
SpaceAroundOperators, SpaceInsideArrayLiteralBrackets,
SpaceInsideHashLiteralBraces, SpaceInsideParens, SpaceInsideReferenceBrackets,
SpaceInsideStringInterpolation, LineContinuationSpacing.

## Principled failures

None yet.
