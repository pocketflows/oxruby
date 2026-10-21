# style-a: Style cops A–E

## Done (whole upstream spec passes; example counts)

ArrayJoin 5, AsciiComments 5, Attr 11, AutoResourceCleanup 7, BeginBlock 1,
BlockComments 5, CharacterLiteral 5, ClassMethods 5, ClassVars 5,
ColonMethodCall 11, ColonMethodDefinition 3, Dir 4, EmptyBlockParameter 9,
EmptyLambdaParameter 3, EndBlock 2, CollectionMethods 68, DigChain 24,
CollectionCompact 28, EmptyStringInsideInterpolation 30, EmptyHeredoc 8,
EmptyLiteral 52.

## Mixins owned (claimed in .coord/claims)

- `string_help` (landed): `StringHelp::candidate` does the `on_str` /
  `on_regexp` guards; the cop keeps `offense?` and `add_offense`.
- `empty_parameter` (landed).
- `interpolation` (landed): `NODE_TYPES` plus `interpolations(node)`; the cop
  calls its own `on_interpolation` for each.
- `string_literals_help` (landed): `wrong_quotes`, `preferred_string_literal`,
  `enforce_double_quotes` (reads `Style/StringLiterals` only when enabled).
- `method_preference` (landed): `preferred_methods` / `preferred_method`
  (merges with `default.yml` as upstream does).
- `allowed_receivers` (landed), `dig_help` (landed).
- Claimed, not landed yet: `endless_method_rewriter`, `documentation_comment`.

## Left

Everything else in Style A–E.

## Blocked / principled failures

None yet.
