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

## Done, waiting on branch `style-a-pending` (they touch shared files)

Main's full gate was frozen by the coordinator; these need a full-gate
landing:

- Encoding 13 (adds `MagicComment#encoding_specified` and `#without`).
- ArrayIntersect 93 (`target_ruby.rs`: a string `TargetRubyVersion` such as
  `'3.1'` is read with Ruby's `to_f`; the spec configures one).
- EndlessMethod 71, AmbiguousEndlessMethodDefinition 31 (new mixin
  `endless_method_rewriter`).
- CommentAnnotation 29, Documentation 61 (+6 pending: rubydex project index,
  as upstream without it) (new mixins `annotation_comment`,
  `documentation_comment`; `Util.regexp_escape`).
- BisectedAttrAccessor 14 (`Investigation::correct`, Base#correct outside
  `add_offense`).

## Left

- CaseLikeIf: needs `min_branches_count` (style-b, not landed yet).
- EmptyElse: needs `on_normal_if_unless` (style-b, not landed yet).
- ClassAndModuleChildren, ComparableClamp: need `alignment` (layout-b, not
  landed yet).
- DocumentationMethod: needs `def_node` (claimed by depts, not written yet).
- Copyright: needs comment tokens (`processed_source.tokens` is a stub).
- DirectiveScope: needs tokens (`comment_only_line?`) and CommentConfig's
  `statement_scope_after` and directive back-references on disabled ranges.

## Mixins owned (claimed in .coord/claims)

Landed: `string_help` (`StringHelp::candidate` does the `on_str`/`on_regexp`
guards), `empty_parameter`, `interpolation` (`NODE_TYPES` +
`interpolations(node)`), `string_literals_help`, `method_preference`,
`allowed_receivers`, `dig_help`.
Pending (on `style-a-pending`): `endless_method_rewriter`,
`annotation_comment`, `documentation_comment`.

## Shared edits

- `magic_comment.rs`, `target_ruby.rs`, `cop/base.rs`
  (`Investigation::correct`), `cop/mixin/util.rs` (`regexp_escape`): pending,
  see above.
- `Regexp.escape` is still duplicated privately in a few of my cops
  (ConditionalAssignment, DocumentDynamicEvalDefinition,
  DisableCopsWithinSourceCodeDirective); switch them to `util::regexp_escape`
  once it lands.

## Blocked / principled failures

None: every ported cop passes its whole spec.
