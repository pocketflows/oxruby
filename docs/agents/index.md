# index: the project index (`AllCops/UseProjectIndex`, rubydex)

Owner: `index` (see the roster in `docs/PARALLEL.md`).

## Status

Done. Every `:project_index` example in the vendored specs runs through the
binary and passes, except two that test the Ruby cop's internals (below).
Suite: 29,116 examples, 0 failures, 5 pending (109 pending before; the
other 3 pending are Layout/IndentationWidth and Lint/UselessAssignment
examples, unrelated). Lint/ArgumentMismatch (24 examples),
Lint/DeprecatedReference (18) and Lint/NameTypo (35) are ported, so all 613
core cops are.

## Design

- `crates/oxcop/src/project_index.rs`: `ProjectIndex`, a resolved
  `rubydex::model::graph::Graph` behind a mutex, with the part of the
  `rubydex` gem's Ruby API RuboCop's cops call (`graph[name]`,
  `resolve_constant`, `Namespace#member/find_member/members/ancestors/
  descendants`, `Declaration#name/definitions/owner/visibility`,
  `Definition#location/comments/deprecated?/superclass/mixins/signatures`,
  `method_references`, `documents`). Each method follows the gem's binding
  (`rubydex-sys/src/*_api.rs`, `ext/rubydex/*.c` at the pinned commit);
  `resolve_constant`'s nesting handling is `rubydex-sys`'s
  `name_from_nesting_names`, redone on the crate's public API (names are
  registered for the lookup and untracked afterwards, as there). Only the
  crate's safe API is used.
- `project_index_loader.rs`: `ProjectIndexLoader.build_index` (rubydex's
  file listing, which keeps only indexable files, then `index_files`, then
  `resolve`; a panic is the "rubydex index build failed" warning) and
  `bundled_gem_source_files`.
- `runner.rs`: `build_project_index` / `project_index_files` /
  `bundled_gem_files` as upstream: when the pwd config has
  `UseProjectIndex: true`, the index covers the target files plus every file
  TargetFinder finds under the config's base dir, plus with
  `ProjectIndexIncludesGems` the bundle's `lib/**/*.rb`. The launcher now
  passes `OXCOP_IN_BUNDLE` (upstream indexes gems only inside a bundle).
  Under `--raw-config` (specs) nothing on disk is indexed.
- The index reaches cops as `cx.project_index()` (`Team::project_index` ->
  `commissioner::investigate` -> `Investigation::project_index`).
- Mixins: `cop/mixin/project_index_help.rs` (all of it except
  `external_dependency_checksum`, which only feeds ResultCache) and
  `cop/mixin/indexed_method_arity.rs`.
- `lockfile.rs`: the gem names of the target's lockfile and which come from
  a `PATH` source (`gem_versions_in_target.keys`,
  `path_sourced_gems_in_target`), for Lint/NameTypo. It reads the file
  `Config::bundler_lock_file_path` (the `config` agent's) finds. Versions
  aren't parsed; nothing else needs them yet. The `config` agent may want
  to fold this into its `gem_versions_in_target` work.
- Spec shim (`spec/oxcop/project_index.rb`): `ProjectIndexLoader.available?`
  is true; `build_index(uri => source)` returns the sources, and the cop
  helper sends them with each investigation (`--project-index-sources FILE`,
  JSON `[[uri, source], ...]`), plus the `:config` context's stubbed
  lockfile gems (`--target-gems FILE`); the binary builds its own graph.
  `project_index_offenses(tmpdir)` runs the binary on the fixture project
  with `-c tmpdir/.rubocop.yml --format json` (`Binary.run_project`).
  `cache_root:` changes nothing (oxcop has no result cache); the cache
  example in constant_reassignment_spec passes because nothing is cached.
- `rake vendor:rubocop` also copies `spec/fixtures/cross_file_*`, the
  fixture projects of those examples (vendored data, like the specs).

## Principled failures

- `Lint/UnusedPrivateMethod` "the reference-name cache" (2 examples,
  pending with a reason): they call the Ruby cop's private
  `referenced_names` and `described_class.cached_reference_names` with a
  real `Rubydex::Graph`, never reaching the binary. oxcop's equivalent is
  `ProjectIndex::reference_names`, computed once per index.

## Decisions / deviations

- A `module_function` method's visibility, which the gem's binding can't
  translate (`unimplemented!`, so RuboCop would crash), is private.
- `util.rs` `interpret_string_escapes`: `contains(c)` became `contains(*c)`;
  linking rubydex brings another `PartialOrd` impl for `char` into scope,
  which broke the inference.

## Measurements (../code, release build, 5,677 files)

`--only Lint/NameTypo,Lint/ArgumentMismatch,Lint/DeprecatedReference` with
a config that inherits code's `.rubocop.yml`:

| AllCops | wall | offenses |
| --- | --- | --- |
| `UseProjectIndex: false` | 0.7 s | 0 |
| `UseProjectIndex: true` | 1.4 s | 1 (NameTypo) |
| plus `ProjectIndexIncludesGems: true` | 3.4 s | 89 |

So building the index over the project costs about 0.7 s, and about 2.7 s
more with every gem's `lib/`. With gems indexed, NameTypo reports names that
gems define outside `lib/` (e.g. `ActiveStorage::Attachment`, which lives in
activestorage's `app/models`), which upstream's `lib/**/*.rb` glob misses
too. Comparing with real RuboCop isn't possible there: the `rubydex` gem is
neither in code's bundle nor installed, and we don't modify code/.

## Left

Nothing in scope. Possible follow-ups: a result cache would need
`external_dependency_checksum` (the index signature); LSP's per-request
index reuse (`reset_project_index`) isn't ported, as oxcop has no LSP server.
