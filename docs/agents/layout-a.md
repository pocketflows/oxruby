# layout-a: Layout cops A–I

## Landed on main (whole upstream spec passes; example counts)

AccessModifierIndentation 41, ArgumentAlignment 55, ArrayAlignment 25,
AssignmentIndentation 10, BlockAlignment 94, BlockEndNewline 18,
CaseIndentation 50, ClassStructure 41, ClosingHeredocIndentation 11,
ClosingParenthesisIndentation 44, CommentIndentation 33,
ConditionPosition 18, DotPosition 40, ElseAlignment 62, EmptyComment 15,
EmptyLineAfterGuardClause 59, EmptyLineAfterMultilineCondition 22,
EmptyLineBetweenDefs 46, EmptyLines 5, EmptyLinesAfterModuleInclusion 61,
EmptyLinesAroundAccessModifier 176, EmptyLinesAroundArguments 22,
EmptyLinesAroundAttributeAccessor 20, EmptyLinesAroundBeginBody 11,
EmptyLinesAroundBlockBody 20, EmptyLinesAroundClassBody 46,
EmptyLinesAroundExceptionHandlingKeywords 24, EmptyLinesAroundMethodBody 14,
EmptyLinesAroundModuleBody 37, ExtraSpacing 94, FirstArgumentIndentation
139, FirstArrayElementIndentation 56, FirstArrayElementLineBreak 14,
FirstHashElementIndentation 60, FirstHashElementLineBreak 11,
FirstMethodArgumentLineBreak 14, FirstMethodParameterLineBreak 11,
FirstParameterIndentation 22, HashAlignment 142 (1 example, the
invalid-style one, raises in the Ruby cop before reaching the binary),
HeredocArgumentClosingParenthesis 82, HeredocIndentation 104,
IndentationConsistency 53, IndentationStyle 25, IndentationWidth 188
(2 pending upstream), InitialIndentation 8. That is 45 of the 50 A–I cops.

The landing carried layout-b's unlanded Alignment / AlignmentCorrector /
EmptyLineCorrector / MultilineLiteralBraceLayout commit (cherry-picked
unchanged, with its 9 cops) and copies of layout-b's `ast/token.rs` and
`mixin/preceding_following_alignment.rs`.
In `preceding_following_alignment.rs` I made four helpers `pub`
(`aligned_comment_lines`, `relevant_assignment_lines`,
`definition_boundary_lines`, `assignment_tokens`) for ExtraSpacing.
**layout-b:** please keep them public.

## Left / blocked

- **`config_to_allow_offenses`** (needs a binary → JSON → shim channel;
  owner `runtime`, same need as depts, style-c, layout-b): BeginEndAlignment,
  DefEndAlignment, EndAlignment (105 of its 216 examples assert it, via
  `spec/support/alignment_examples.rb`). All three are ported and pass
  everything else; they are parked on `layout-a-wip`. The Ruby side would also
  need `ConfigurableEnforcedStyle`'s detection calls (`correct_style_detected`,
  `opposite_style_detected`, ...), which the Rust mixin doesn't port yet.
- **ValidationError from `validate_config`** (owner `runtime`: the binary
  must run `validate_config` and the shim must raise
  `RuboCop::ValidationError`): EmptyLineAfterMagicComment (4 examples).
- **Non-UTF-8 source** (`# encoding: iso-8859-15` files fail with "stream did
  not contain valid UTF-8"; owner `runtime`/processed source): EndOfLine
  (4 of 17 examples). Ported, parked on `layout-a-wip`.
- EmptyLineAfterMagicComment is ported too (26 of 30 pass), on
  `layout-a-wip`.

## Shared helpers I own

- `cop/mixin/check_assignment.rs`: `NODE_TYPES` and `assignment(node)`
  give the `check_assignment(node, rhs)` call the Ruby mixin would make.
  `IndentationWidth` aliases `on_csend` to `on_send`, so it calls
  `extract_rhs` itself for `csend`.
- `cop/mixin/end_keyword_alignment.rs` (style parameter
  `EnforcedStyleAlignWith`; the cop's ignored nodes and autocorrect are
  passed in).
- `cop/mixin/empty_lines_around_body.rs` (`Body { kind, style }`).
- `cop/mixin/first_element_line_break.rs`.
- `cop/mixin/multiline_element_indentation.rs`.
- `cop/mixin/hash_alignment_styles.rs` (also the rubocop-ast
  `HashElementNode` deltas; `forwarded_kwrestarg` counts as a kwsplat, as
  rubocop-ast's `KeywordSplatNode`).
- Small additions: `Node::is_single_line_condition`, `Config::for_enabled_cop`
  (layout-b's identical version).

## Notes

- Style detection (`correct_style_detected` and friends) is not ported in
  any of these cops; it only feeds `--auto-gen-config`.
- `IgnoredNodes` is reset in `on_new_investigation` (Ruby's identity
  matching never matches nodes of a re-parsed source), while memoized state
  Ruby keeps for the cop's lifetime (`IndentationWidth`'s
  `@offense_ranges`, `ExtraSpacing`'s `@ignored_ranges`) is kept, which
  matters in the `--persist-cops` correction loop.
