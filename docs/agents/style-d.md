# style-d: Style cops S–Z

## Done (whole upstream spec passes; example counts)

Landed on main: Send 13, SymbolLiteral 4, TimeNow 11,
SuperWithArgsParentheses 4, StringChars 8, Strip 6, StderrPuts 5,
StringHashKeys 11, UnpackFirst 11, YAMLFileRead 8, WhenThen 4,
WhileUntilDo 10, SafeNavigationChainLength 8, StructInheritance 18,
SelfAssignment 105, SlicingWithRange 27, TopLevelMethodDefinition 14,
TrailingMethodEndStatement 11, SwapValues 11, YodaExpression 10,
ZeroLengthPredicate 65, Sample 79, SendWithLiteralMethodName 41,
SelectByKind 144, SelectByRegexp 256, SelectByRange 120,
StabbyLambdaParentheses 9, SingleLineBlockParams 12, StringConcatenation 31,
SignalException 27, UnlessLogicalOperators 23, TallyMethod 30,
SuperArguments 92, TrivialAccessors 39, TernaryParentheses 99,
VariableInterpolation 9, StringMethods 2, SingleArgumentDig 15,
YodaCondition 72, TrailingUnderscoreVariable 62.

Second batch (full gate): SafeNavigation 797, SoleNestedConditional 74,
SingleLineDoEndBlock 17, StaticClass 14 (3 pending: project index),
TrailingCommaInArguments 178, TrailingCommaInArrayLiteral 48,
TrailingCommaInHashLiteral 41, SingleLineMethods 138, SymbolProc 77,
Semicolon 39, TrailingCommaInBlockArgs 20, TrailingBodyOnClass 7,
TrailingBodyOnModule 7, TrailingBodyOnMethodDefinition 12.

Third batch: WhileUntilModifier 50, SpecialGlobalVars 31, SymbolArray 33,
StringLiterals 58, StringLiteralsInInterpolation 13 (style detection feeds
`config_to_allow_offenses`; an unsupported `EnforcedStyle` raises
`Unknown style ... selected!` as upstream's `style` does).

Shared changes: `util::symbol_inspect`; `util::ruby_inspect` escapes control
characters as `\uXXXX` (Ruby's `String#inspect` for UTF-8); `prism.rs`
parses with `partial_script: true`, as `Prism::Translation::Parser` does
(top-level `yield` is not a syntax error).

## Mixins and correctors owned

Mixins: NilMethods, ReparsedEquivalence (whole mixin: `correction_parses`,
`verified_by_reparse`; Ruby's `normalize_reparsed_ast` hook is the trait's
`reparsed_equal`), CheckSingleLineSuitability (with `max_line_length`),
TrailingComma, TrailingBody, RequireLibrary, PreferredDelimiters,
PercentArray, ArrayMinSize (no `array_style_detected`: auto-gen-config only),
ArraySyntax. Correctors: PunctuationCorrector, PercentLiteralCorrector,
RequireLibraryCorrector, StringLiteralCorrector, LineBreakCorrector.

## Left / blocked

Every S–Z cop is landed except Style/WordArray (55 of 60 pass; file on
local branch `style-d-wip`):
- `:85` (US-ASCII default external encoding): RuboCop's correction depends
  on the spec process's `Encoding.default_external` (`String#inspect`
  escapes non-ASCII), which the binary can't see. Principled.
- `:324` (two cop instances): relies on the class-level
  `WordArray.largest_brackets` and on `config_to_allow_offenses` carried
  from one `_investigate` to the next through `DisabledConfigFormatter`.
  The binary starts each request with empty auto-gen state.
- `:294`, `:309`: assert `config_to_allow_offenses` after
  `expect_correction`. Upstream's state accumulates across the correction
  loop (it lives in the formatter, keyed by cop name); `runner.rs` keeps
  only the last iteration's state, so the final all-`%w` source overwrites
  it. For `runtime`: carry `AutoGenState` across iterations of the
  `--persist-cops` loop (and merge rather than replace in `runner.rs`).
  SymbolArray's equivalent examples don't call `expect_correction`, so it
  passes.

`ArrayMinSize#array_style_detected` is ported; `largest_brackets` lives in
the cop's `ArrayStyleState`, not on the class.
