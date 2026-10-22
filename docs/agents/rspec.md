# rspec agent (Round 3)

Ports rubocop-rspec 3.10.2 cops from `../rubocop-rspec`.

## Done

| Cop | Vendored spec |
| --- | --- |
| `RSpec/EmptyLineAfterHook` | 51 examples, 0 failures (smoke tests included) |
| `RSpec/EmptyLineAfterExample` | 17 examples, 0 failures (smoke tests included) |

Also checked in a real run (`plugins: [rubocop-rspec]` in a scratch
project): both cops fire and read `RSpec: Language:` from the plugin's
default config.

## Infrastructure for future RSpec cops

All under `crates/oxcop/src/cop/rspec/` (helper subdirectories, so build.rs
exposes them as `crate::cop::rspec::<dir>`):

- `language/mod.rs`: port of `lib/rubocop/rspec/language.rb` plus
  `RSpec::Base#on_new_investigation`. `Language::new(cx.config)` reads
  `config['RSpec']['Language']` (the department, as upstream; not the cop
  config). Matchers: `rspec`, `explicit_rspec`, `example_group`,
  `shared_group`, `spec_group`, `example_group_with_body`, `example`, `hook`,
  `let_`, `include`, `subject`, with the upstream patterns verbatim.
  Concepts: `example_groups[_regular|_focused|_skipped]`,
  `examples[_regular|_focused|_skipped|_pending]`, `shared_groups[...]`,
  `includes[...]`, `helpers`, `hooks`, `subjects`, `expectations`,
  `error_matchers`, `HOOK_SCOPES`, `RUNNERS`, and `all` (`ALL.all`, returns a
  `Concept`). A cop's own NodePattern that uses `#rspec?`, `#Examples.all` etc.
  can resolve them with `language.matches(&PATTERN, val)`, or from its own
  `MatchContext` call closure by delegating to `language.call(name, val)`.
  A missing Language section means nothing matches (upstream would raise).
- `mixin/final_end_location.rs`: `final_end_location(node) -> Option<Range>`
  (`None` when the node has no `loc.end`, where upstream raises).
- `mixin/empty_line_separation.rs`: `missing_separating_line_offense(cx,
  node, |method| msg)`, `missing_separating_line`, `offending_loc`,
  `last_child`. Uses core `range_help::source_range` and
  `DirectiveComment` (no core RangeHelp changes needed).

Not ported yet (not needed by these two cops): `InsideExampleGroup`,
`InsideExample`, `TopLevelGroup`, `Metadata`, `Variable`, `CommentsHelp`,
`FileHelp`, `LocationHelp`, `Namespace`, `RepeatedItems`, `SkipOrPending`
(all in `../rubocop-rspec/lib/rubocop/cop/rspec/mixin/`). Add them as files
under `rspec/mixin/` and list them in its `mod.rs`.

## Shared files touched

None. Everything lives under `cop/rspec/**` and this file.

## Principled spec failures

None.

## Notes / decisions for the user

- `EmptyLineAfterHook` includes `ConfigurableEnforcedStyle` upstream but
  never uses it; not ported.
- rubocop-rspec's default config has `inherit_mode: merge:` *inside*
  `RSpec: Language:` (for Expectations, Helpers, Hooks, Subjects). I did not
  check whether oxcop's config loader honours a nested `inherit_mode` when a
  user's `.rubocop.yml` adds `RSpec: Language: Hooks: [...]`; worth a look
  when the config gaps are next worked on (upstream merges those lists with
  the defaults).

## Left

Scope for this round is done. Next candidates sharing this infrastructure:
`RSpec/EmptyLineAfterExampleGroup`, `EmptyLineAfterFinalLet`,
`EmptyLineAfterSubject` (all use `EmptyLineSeparation`, some
`InsideExampleGroup`).
