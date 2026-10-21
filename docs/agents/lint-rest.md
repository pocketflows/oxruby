# lint-rest: the last Lint cops

Scope: the ten Lint cops lint-a left blocked (see `lint-a.md`), now that
tokens, regexp_parser, CheckAssignment, Alignment and NameSimilarity are on
main. ArgumentMismatch, DeprecatedReference and NameTypo are excluded: they
only work with a `rubydex` project index (decision pending with the user).

## Done (whole upstream spec passes; example counts)

- AmbiguousAssignment 88 (CheckAssignment, plus `on_csend`)
- ElseLayout 12 (Alignment's `indentation`)
- MisplacedMagicComment 16 (`sorted_tokens`; `known_encoding?` uses Ruby
  3.3's `Encoding.name_list`, downcased, since `Encoding.find` is
  case-insensitive)
- DuplicateMagicComment, OrderedMagicComments (21 together;
  `FrozenStringLiteral#leading_magic_comments` added to the mixin)
- DuplicateRegexpCharacterClassElement 16, MixedCaseRange 31,
  MixedRegexpCaptureTypes and OutOfRangeRegexpRef (133 together)
- CopDirectiveSyntax 51. The registry is `config.known_cops()` (every core
  cop from `default.yml`, plus cops the config names), so unported cops are
  known too. `Registry#qualified_cop_name(name, nil, warn: false)` (with
  namespace correction, `Badge.parse` camel-casing, and the
  `AmbiguousCopName` raise) and `contains_cop_matching?` are ported inside
  the cop. `comment_only_line?` comes from `comment_config::DirectiveScopes`.

All ten match RuboCop 1.91.0 (`--only` these cops, a plain config with
`NewCops: enable`) on ../code and on copies of rubocop's `lib/` and `spec/`
(97 offenses, 0 mismatched files); `-A` changes nothing in either.

## Shared changes

- `cop/mixin/frozen_string_literal.rs`: `leading_magic_comments`.
- `directive_comment.rs` (owned by `tokens`): `AVAILABLE_MODES` and
  `modes_pattern()` are public (`MODES_PATTERN`), for CopDirectiveSyntax's
  near-miss keyword regexp.

## Notes

- NameSimilarity ranks `known_cops().names()`, which are sorted; Ruby's
  `Registry#names` is in registration order. Only exact ties could differ.

## Left

Nothing in scope.
