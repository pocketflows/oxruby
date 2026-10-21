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

## Directive cops (done)

Lint/MissingCopEnableDirective 16/16, Lint/RedundantCopEnableDirective
28/28, Lint/RedundantCopDisableDirective 65/65. The runner runs
RedundantCopDisableDirective after the inspection loop with every
offense (disabled ones included) as `offenses_to_check`, then one more
loop when it removed directives (`Runner#add_redundant_disables`; skipped
with `--only`, as upstream). Checked against rubocop on a sample with
`-A`: same offenses and correction.

- `Config::known_cops()` is the global registry: every core cop plus the
  configuration's own (plugin/custom) cops.
- `processed_source.registry` (for `disabled_names`): the cops `--only` /
  `--except` mobilize; under `--raw-config`, the cops and departments the
  spec's configuration names (CopHelper#registry).
- `cli::warn(msg)` prints a warning and records it for `--spec-report`; the
  shim prints them to the spec's `$stderr` (not for replayed
  investigations: `--prior`, `--quiet-first-pass`, the final re-investigation
  of oxcop's corrected source). CommentConfig prints "no department given"
  through it.
- Shim edits made for these (small, in `spec/oxcop/`, now `shim`'s):
  `offenses_to_check` passed as `--offenses-to-check FILE`; warnings
  forwarding; `--quiet-first-pass` on autocorrect calls.

## Handoff to `shim`

Already landed by runtime (9fe3cd9 and before), so `shim` needn't redo
them: AutoGenState persistence across the `--persist-cops` loop and across
calls in one example; `--lsp`; re-raising cop errors as their class
(RuboCop::Warning, ValidationError, ...); global offenses on an empty
buffer as `NO_LOCATION`; ExcludeLimit tmp_dir; `buffer.name` file names;
cop instances persisting across files in one example (`--prior`);
real-path inspection (ScriptPermission's shape); binary-encoded and
magic-encoding stdin; the "no department given" warning (CommentConfig,
forwarded to `$stderr`). Not done: Migration/DepartmentName itself,
invalid-UTF-8 strings in a spec's cop config
(`YAML.dump` raises; serde can't hold the bytes anyway).

## API for cop authors (shim support)

- **Raising from a cop** (`raise ArgumentError, "..."` upstream): call
  `crate::cop::base::raise("ArgumentError", message)`; a plain `panic!("msg")`
  is a `RuntimeError` with that message. The Commissioner rescues it per
  callback (`with_cop_error_handling`) and the run goes on; the error is
  printed as RuboCop does ("An error occurred while X cop was inspecting
  file:line:col." or, for `RuboCop::Warning`, "msg (from file: ...)").
  In specs the shim re-raises the first error as that class (any constant
  name works: `RuboCop::Warning`, `RuboCop::ValidationError`, ...), so
  `expect { ... }.to raise_error(...)` passes.
- **`config_to_allow_offenses`**: `cx.config_to_allow_offenses()` gives the
  `&mut Mapping`; `cx.auto_gen.detected_style` is `detected_style`.
  `configurable_enforced_style::{correct_style_detected,
  opposite_style_detected, unexpected_style_detected,
  ambiguous_style_detected, style_detected, no_acceptable_style,
  no_acceptable_style_bang, conflicting_styles_detected,
  unrecognized_style_detected}` take the style parameter name
  (`"EnforcedStyle"` for the default `style_parameter_name`). The shim
  copies the state into `DisabledConfigFormatter` after each offenses call,
  so `cop.config_to_allow_offenses` works.
- **`exclude_limit`** setters (`self.max = n`): `cx.set_exclude_limit("Max",
  n)` where upstream calls the setter. The shim appends to
  `ExcludeLimit.tmp_dir`, so `read_exclude_limit(cop, 'Max')` works.
- **`validate_config`**: implement `Cop::validate_config(&self, cx) ->
  Result<(), String>`; `Err(message)` is `raise ValidationError, message`.
  The team checks it before each file; the run stops with `Error: message`
  (exit 2) and the shim raises `RuboCop::ValidationError`.
- **Source encodings** (`source_encoding.rs`): files and stdin are read as
  bytes. A magic `encoding:` comment is honoured as
  `Parser::Source::Buffer` does: `BINARY` keeps one character per byte
  (U+00XX, written back as those bytes), other encodings are transcoded to
  UTF-8 (encoding_rs). No comment means UTF-8 (invalid bytes are replaced;
  RuboCop reports such files as unparsable instead).
- **State carries over** as upstream's class-level hashes do: the team
  keeps each cop's `AutoGenState` across the autocorrect loop, and the shim
  passes the current Ruby state in (`--auto-gen-state FILE`), so
  `expect_offense` + `expect_correction` accumulate (FormatStringToken
  420/420 with style-b-wip's port). Cops must still *call* the detection
  functions: layout-a-wip's EndAlignment/DefEndAlignment/BeginEndAlignment
  fail only because their ports don't call `correct_style_detected` /
  `opposite_style_detected` / `unrecognized_style_detected` yet.
- **One cop object per example**: the shim replays the example's earlier
  investigations first (`--prior NAME=FILE`), so cop state carries over
  between files as upstream's memoized `cop` does (DuplicateMethods: only
  its class_eval example and the two `Dir.pwd`-stubbing ones fail).
- **Buffer name**: the shim sends `processed_source.buffer.name` and
  `--keep-stdin-path`, so `file_path` is the name as given (GemFilename
  15/15 with depts-wip's port).
- **Real files**: when the spec wrote the source to an existing file
  (`expect_offense(src, file)`) and the cop's options have no `:stdin`, the
  binary inspects the file itself (no `--stdin`), and `cx.stdin()` is
  false, as Lint/ScriptPermission needs. A cop whose `@options[:autocorrect]`
  is `false` gets no corrections. `cx.stdin()` / `cx.lsp()` mirror
  `@options.key?(:stdin)` / `RuboCop::LSP.enabled?` (hidden `--lsp`).
- **Global offenses on an empty buffer** come back as `Offense::NO_LOCATION`.
- Plumbing: hidden `--spec-report FILE` writes `{config_to_allow_offenses,
  detected_styles, exclude_limits, raised}` as JSON (`runner::SpecReport`).

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
