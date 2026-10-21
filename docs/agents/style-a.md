# style-a: Style cops A–E

## Landed on main (whole upstream spec passes; example counts)

AccessModifierDeclarations 400, AccessorGrouping 27, Alias 35, AndOr 80,
ArgumentsForwarding 122, ArrayCoercion 5, ArrayFirstLast 18, ArrayJoin 5,
ArrayIntersectWithSingleElement 5, AsciiComments 5, Attr 11,
AutoResourceCleanup 7, BarePercentLiterals 36, BeginBlock 1,
BitwisePredicate 15, BlockComments 5, BlockDelimiters 177, CaseEquality 27,
CharacterLiteral 5, ClassCheck 4, ClassEqualityComparison 24,
ClassMethods 5, ClassVars 5, CollectionCompact 28, CollectionMethods 68,
CollectionQuerying 20, ColonMethodCall 11, ColonMethodDefinition 3,
CombinableDefined 39, CombinableLoops 22, CommandLiteral 35,
CommentedKeyword 32, ComparableBetween 15, ConcatArrayLiterals 17,
ConditionalAssignment 1227 (two spec files: `conditional_assignment_assign_in_condition_spec.rb`
and `..._assign_to_condition_spec.rb`; `script/land` maps the cop file to a
spec name that doesn't exist, so it runs neither), ConstantVisibility 18,
DataInheritance 18, DateTime 14, DefWithParentheses 11, DigChain 24, Dir 4,
DirEmpty 11, DisableCopsWithinSourceCodeDirective 37,
DocumentDynamicEvalDefinition 20, DoubleNegation 47, EachForSimpleLoop 20,
EachWithObject 16, EmptyBlockParameter 9, EmptyCaseCondition 30,
EmptyClassDefinition 53, EmptyHeredoc 8, EmptyLambdaParameter 3,
EmptyLiteral 52, EmptyMethod 34, EmptyStringInsideInterpolation 30,
EndBlock 2, EnvHome 8, EvalWithLocation 27, EvenOdd 21, ExactRegexpMatch 15,
ExpandPathArguments 16, ExplicitBlockArgument 21, ExponentialNotation 27.

Notes:
- ExactRegexpMatch checks `\A<literal>\z` directly instead of through a
  `regexp_parser` port (none exists yet); the spec passes in full.
- ConditionalAssignment's `safe_to_correct?` (ReparsedEquivalence's
  `correction_parses?`) applies the correction and reparses with Prism,
  locally; it doesn't use style-d's `reparsed_equivalence` mixin.

## Landed in the second batch (after the gate freeze)

AmbiguousEndlessMethodDefinition 31, ArrayIntersect 93, BisectedAttrAccessor 14,
CaseLikeIf 38, ClassAndModuleChildren 60 (+7 pending: rubydex project index),
CombinableDefined 39, CommentAnnotation 29, ComparableClamp 20,
Documentation 61 (+6 pending: rubydex), EmptyElse 124, Encoding 13,
EndlessMethod 71.

- CaseLikeIf's `regexp_with_named_captures?` looks for `(?<name>` / `(?'name'`
  in the regexp's literal parts (written before the regexp_parser port
  landed; it could now use `RegexpNode#parsed_tree`).

## Left

- DocumentationMethod: needs `def_node` (claimed by depts, not on main yet).
- Copyright: needs comment tokens (now landing).
- DirectiveScope: needs tokens (`comment_only_line?`) plus public
  CommentConfig `comment_only_line` / `statement_scope_after` (the analyzer
  has them privately; `cop_disabled_line_ranges` and `directives` are public).

## Mixins owned (claimed in .coord/claims)

Landed: `string_help` (`StringHelp::candidate` does the `on_str`/`on_regexp`
guards), `empty_parameter`, `interpolation` (`NODE_TYPES` +
`interpolations(node)`), `string_literals_help`, `method_preference`,
`allowed_receivers`, `dig_help`.
Also landed: `endless_method_rewriter`, `annotation_comment`,
`documentation_comment`.

## Shared edits

- `magic_comment.rs`: `encoding_specified`, `without(type)`.
- `cop/base.rs`: `Investigation::correct` (Base#correct outside add_offense).
- `cop/mixin/util.rs`: `regexp_escape` (Regexp.escape); my cops use it.

## Notes for the coordinator / runtime

- `script/land` gates cop-only changes by mapping `cop/style/x.rs` to
  `x_spec.rb`; ConditionalAssignment's specs have other names, so its landing
  ran none (both pass locally: 1227 examples).
- After a rebase race, the BlockDelimiters landing log showed `77 examples`
  and `compare_with_rubocop (Style/SymbolProc)`: the retry seems to gate the
  delta main gained, not this branch's cop (177 examples pass locally).

## Blocked / principled failures

None: every ported cop passes its whole spec.
