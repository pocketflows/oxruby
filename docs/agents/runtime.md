# runtime: notes

Owner: `runtime` (see the roster in `docs/PARALLEL.md`).

## Done

- **Spec run speed.** The shim now sends every binary call to one
  long-lived `oxcop --server` per rspec process (`crates/oxcop/src/server.rs`,
  `Oxcop::Spec::Server` in `spec/oxcop/binary.rb`). Spawning from the spec
  process was the cost: with all ~600 spec files loaded its heap is large,
  and each `Open3.capture3` took ~25 ms (the binary itself runs in ~3 ms).
  Full `bundle exec rspec`: 46 s before, 6.4 s after (919 examples).
  - Same semantics as before: requests carry the exact argv (so
    `--raw-config`, `--persist-cops`, `--autocorrect-once` are unchanged),
    the cwd, and stdin bytes (length-prefixed, so invalid UTF-8 is fine).
    Each request runs `cli::run_with` with buffered stdout/stderr; a panic
    becomes that request's stderr and exit status 101.
  - Falls back to a fresh process when `ENV` differs from the server's
    (the binary reads `HOME`, `XDG_CONFIG_HOME`, ...), or when the server
    dies (it is restarted on the next call). `OXCOP_NO_SERVER=1` always
    spawns.
  - Output written deep inside a run goes through `cli::stderr()` (a
    thread-local sink), not `eprintln!`. **Other agents:** don't
    `eprintln!` from cops; report errors through the investigation's
    `errors`, or write to `crate::cli::stderr()`.

- **`--only`** enables a cop the config disables, only when named exactly
  (not by department), as `Registry#enabled_cop_name?` does.
- **Gate:** cop-only landings run `compare_with_rubocop` restricted to the
  changed cops (`ONLY=Dept/Cop,...`). Compare must exit 0 (no mismatches
  and no files only one side inspects). On a CAS retry, if main only gained
  cop/docs files, only those are re-gated on top of the rebased branch.
- **Plugin default configs.** The launcher passes `OXCOP_GEMS` (every gem
  in the bundle, or the latest installed versions: name, path,
  `default_lint_roller_plugin`). `plugins:` (and `require:` of a plugin gem)
  load `<gem>/config/default.yml` and are integrated into a per-run default
  configuration as `ConfigurationIntegrator` does (first-in-wins AllCops,
  arrays unioned, `unset_nil: false`). Without the launcher the old warning
  is kept; with it, an unknown plugin is RuboCop's `Plugin::LoadError`.
  `compare_with_rubocop` runs the launcher with `bundle exec ruby` inside
  ../code's bundle: `rubocop=5692 oxcop=5692`, 0 mismatched, exit 0.
- **`inherit_mode`** (root, hash-level and per-cop; merge and override) in
  `merge`, `resolve_inheritance` ported (department overrides, Include path
  fixing), `merge_with_default` uses the user's `inherit_mode`.

- **Parallel inspection** (rayon), on by default as in RuboCop
  (`--no-parallel` turns it off; `RAYON_NUM_THREADS` caps it). Configs
  are resolved first, files inspected on the pool, then reported in file
  order, so output is byte-identical to `--no-parallel` (checked on
  ../code app/ + lib/: 3.1 s vs 11.9 s, debug). `--stdin` stays serial.

- **ERB** in config files: rendered by `ruby -rerb` in the file's
  directory (`OXCOP_RUBY` from the launcher, else `ruby` on PATH), only
  when the file contains `<%`. **`inherit_gem`** resolves through
  `OXCOP_GEMS`; without the launcher it warns and is skipped.

## Left

- Debug oxcop on the full ../code compare was ~225 s with ~170 cops, and
  almost all of it is **Style/RedundantFreeze**: 55 s on app/models alone
  (every other cop < 1 s). It calls `frozen_string_literals_enabled` per
  `.freeze`, which re-reads the leading comments through `MagicComment`
  each time (the coordinator has a MagicComment perf fix pending).
- Plugins whose `rules` aren't `config/default.yml` (`type: :object`).
- `inherit_from` globs, remote configs,
  `handle_disabled_by_default`, `add_missing_namespaces`,
  `add_excludes_from_files`, ConfigFinder's project-root stop.
- Directive cops, pending-cops warning, CLI options, result cache, rayon.
- Parallel spec files (parallel_tests), once the suite grows enough that
  per-example Ruby time dominates.

## Gaps / blockers

None yet.
