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

- Needs `cop.config_to_allow_offenses` through the spec shim (`runtime`,
  `spec/oxcop/`): SpecialGlobalVars (4 of 31 examples), SymbolArray (2 of 33),
  WordArray (5 of 60), StringLiterals (3 of 58). StringLiterals and
  StringLiteralsInInterpolation also have a "bad EnforcedStyle raises
  RuntimeError" example: the shim would need to raise when the binary
  reports a cop error.
  These are on local branch `style-d-wip`.
- WhileUntilModifier: needs StatementModifier (claimed by `style-b`).
- Note: `Alignment#configured_indentation_width` is copied privately into
  SingleLineMethods and the TrailingBody cops until `layout-b` lands
  `mixin/alignment.rs`.
