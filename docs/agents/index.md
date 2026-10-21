# index: the project index (`AllCops/UseProjectIndex`, rubydex)

Owner: `index` (see the roster in `docs/PARALLEL.md`).

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
  registered for the lookup and untracked afterwards, as there).
- `project_index_loader.rs`: `ProjectIndexLoader.build_index` (rubydex's
  file listing, then `index_files`, then `resolve`; a panic is the
  "rubydex index build failed" warning) and `bundled_gem_source_files`.
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
- Spec shim (`spec/oxcop/project_index.rb`): `ProjectIndexLoader.available?`
  is true; `build_index(uri => source)` returns the sources, and the cop
  helper sends them with each investigation (`--project-index-sources FILE`,
  JSON `[[uri, source], ...]`), from which the binary builds its own graph;
  `project_index_offenses(tmpdir)` runs the binary on the fixture project
  with `-c tmpdir/.rubocop.yml --format json`. `INDEX_AWARE_COPS` lists the
  cops whose index code is ported; the others' `:project_index` examples stay
  pending until they're added.

## Decisions / deviations

- A `module_function` method's visibility, which the gem's binding can't
  translate (`unimplemented!`, i.e. RuboCop would crash), is private.
- `util.rs` `interpret_string_escapes`: `contains(c)` became `contains(*c)`;
  linking rubydex brings another `PartialOrd` impl for `char` into scope,
  which broke the inference.

## Done

- Infrastructure above; Style/StaticClass's `subclassed_in_project?`.

## Left

- Lint/ArgumentMismatch, Lint/DeprecatedReference, Lint/NameTypo.
- The index-aware paths of the other cops with `:project_index` examples.
