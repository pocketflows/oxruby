# style-a: Style cops A–E

## Done (whole upstream spec passes; example counts)

ArrayJoin 5, AsciiComments 5, Attr 11, AutoResourceCleanup 7, BeginBlock 1,
BlockComments 5, CharacterLiteral 5, ClassMethods 5, ClassVars 5,
ColonMethodCall 11, ColonMethodDefinition 3, Dir 4, EmptyBlockParameter 9,
EmptyLambdaParameter 3, EndBlock 2.

## Mixins owned (claimed in .coord/claims)

- `string_help` (landed): `StringHelp::candidate` does the `on_str` /
  `on_regexp` guards; the cop keeps `offense?` and `add_offense`.
- `empty_parameter` (landed).
- Claimed, not landed yet: `endless_method_rewriter`, `string_literals_help`,
  `interpolation`, `method_preference`, `allowed_receivers`, `dig_help`,
  `documentation_comment`.

## Left

Everything else in Style A–E.

## Blocked / principled failures

None yet.
