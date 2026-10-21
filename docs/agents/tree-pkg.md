# tree-pkg: oxtree tests, gem, rake tasks and CI

Scope: OXTREE.md "What's left" items 2 (gem and rake tasks), 4 (ERB and
plugin tests in `cargo test`) and 5 (`rake oxtree:check`, CI).

## Done

### Tests (`crates/oxtree/tests/`)

`cargo test -p oxtree` now runs 93 tests (6 lib unit tests, 6 in
`fixtures.rs`, 61 in `erb.rs`, 20 in `plugins.rs`), all passing.

- `erb.rs`, a port of w_syntax_tree-erb 0.12.0's tests:
  - `formatting_test.rb`: all 7 `test/fixture/*` pairs, each checked for
    output and idempotency. `every_fixture_is_tested` fails if upstream adds a
    fixture without a test.
  - `erb_test.rb` and `html_test.rb`: every `assert_formatting` pair (formats,
    reformats, and the expected output parses) and every parse-error case
    (message, line and column).
  - Stricter than upstream: upstream's parse-error tests assert only inside
    `rescue`, so they pass when nothing raises. Here the error is required.
  - Node-class assertions are ported, not skipped: the instructions said to
    skip them, but the ERB tree is public (`oxtree::erb::parser::Elem`), so
    `assert_instance_of(ERB::Doctype, ...)` becomes `matches!(.., Elem::Doctype
    { .. })`, and the location, attribute and `new_line` checks map directly.
  - Skipped, because oxtree has no equivalent:
    - `Document#location` (`assert_nil(parsed.location)` in
      `test_empty_file`);
    - the `pretty_inspect` half of `assert_formatting`, which is replaced by
      "the expected output parses".
- `plugins.rs`: syntax_tree 6.3.0's `test/plugin/` (single_quotes,
  trailing_comma, disable_auto_ternary) against `format::Options` at width
  80. It also checks that the auto-ternary rewrite happens without the plugin,
  so the disable test can't pass vacuously.

### Gem (`gems/oxtree/`)

A copy of `gems/oxcop`'s layout: `exe/oxtree` launcher, `lib/oxtree.rb`
(platform matching through `Gem::Platform.match_gem?` over
`exe/<platform>/oxtree`, then `target/{release,debug}/oxtree` in a checkout,
`OXTREE_BINARY` override), `oxtree.gemspec` with the same authors, license
and homepage, and `lib/oxtree/version.rb` (`VERSION`, plus
`SYNTAX_TREE_VERSION = "6.3.0"`, `SYNTAX_TREE_ERB_VERSION = "0.12.0"`).

- The launcher passes every argument straight to the binary. It doesn't set
  the `OXCOP_RUBY_*` environment that oxcop's launcher sets, because nothing
  in oxtree reads it. Ask for it if the CLI needs the Ruby version.
- **`stree` executable (decision pending with the user):** opt-in at gem
  build time only. `OXTREE_STREE=1 rake oxtree:gem:...` adds `exe/stree` to
  the gem's files and executables. RubyGems makes bin stubs at install time
  from `spec.executables`, so it can't be a run-time switch. The published
  gems would need one of: build with `OXTREE_STREE=1` (every user gets an
  `stree` that shadows syntax_tree's), publish a separate `oxtree-stree` gem,
  or no `stree` at all (the default now).

### Rake (`rakelib/oxtree.rake`)

Rake loads it after the Rakefile, so it reuses `ROOT`, `PKG_DIR`,
`NATIVE_PLATFORMS` and `cargo_version`. Everything is under `oxtree:`:

| Task | What it does |
| --- | --- |
| `oxtree:build` | `cargo build -p oxtree` |
| `oxtree:check` | `cargo fmt -p oxtree --check`, clippy `-p oxtree --all-targets -D warnings`, `cargo test -p oxtree` |
| `oxtree:version:check` | Cargo.toml's version matches `gems/oxtree` |
| `oxtree:gem:ruby` | the plain `ruby` gem in `pkg/` |
| `oxtree:gem:native[platform]` | the native gem. Uses `BINARY=` or `cargo build --release -p oxtree --target` |
| `oxtree:vendor` | re-syncs `vendor/syntax_tree{,-erb}` from the sibling clones (it finds them next to the main checkout when run from a worktree), warns if a clone's tag doesn't match `version.rb`, and rewrites the vendor READMEs. Run at v6.3.0/v0.12.0, it leaves no diff. |

`task default: "oxtree:version:check"` is added. The Rakefile's `cargo:check`
is already workspace-wide, so `bundle exec rake` covers oxtree's fmt, clippy
and tests without running `oxtree:check` a second time.

### Gem build and install (arm64-darwin, local only, nothing published)

- `rake "oxtree:gem:native[arm64-darwin]"` runs `cargo build --release -p
  oxtree --target aarch64-apple-darwin` (39 s with a warm cache) and builds
  `pkg/oxtree-0.1.0-arm64-darwin.gem` (1.8 MB, 4.2 MB binary). Its files are
  `exe/oxtree`, `exe/arm64-darwin/oxtree`, `lib/**`, LICENSE.txt and
  README.md. The executable is `oxtree` only.
- `rake oxtree:gem:ruby` builds `pkg/oxtree-0.1.0.gem` (no binary).
- Installed the native gem with `gem install --local` into a scratch
  GEM_HOME. `bin/oxtree --format-file` and `--format-erb` format correctly
  through the packaged binary.
- Installed the `ruby` gem the same way. `bin/oxtree` prints "oxtree does not
  ship a prebuilt binary for arm64-darwin-24 ..." and exits 2. With
  `OXTREE_BINARY=...` it runs.
- With `OXTREE_STREE=1` the native gem also installs `bin/stree`, which runs
  the same binary.
- `bundle exec rake version:check cargo:check oxtree:version:check` (the
  default task minus the rspec run) passes with both crates, and so does
  `rake oxtree:check`.

### CI (`.github/workflows/`)

**Not landed yet:** this is commit 0d31dcd on the local branch `tree-pkg-wip`.
It touches `.github/`, so `script/land` runs the full gate. On 2026-10-21
rspec passed (2004 examples, 0 failures), but `compare_with_rubocop ../code`
reported 24 mismatched files on main 72921c9. That came from the Style cops
landed just before it, not from this YAML-only diff. Cherry-pick it onto
`tree-pkg` and land once main's compare is back to 0.

Additive lines only:

- `ci.yml`: `cargo build -p oxtree --no-default-features`, the
  oxfmt-executable-only build that nothing else compiles. oxtree's tests
  already run in `bundle exec rake`.
- `release.yml`: an `oxtree:gem:native[platform]` step with
  `BINARY=target/<target>/release/oxtree`. The workspace-wide `cargo build
  --release --target` already builds that binary. The publish job also gets
  `rake oxtree:gem:ruby`, and its `pkg/*.gem` push loop picks up the oxtree
  gems. Not exercised: the release workflow has never run.

## Decisions for the user

- **`stree` executable:** see "Gem" above. It's off unless the gem is built
  with `OXTREE_STREE=1`.
- **Publishing oxtree on the next tag:** with the release.yml change, a `v*`
  tag would `gem push` oxtree 0.1.0, whose binary has only dev commands until
  tree-cli's CLI lands. Decide whether to gate the `oxtree:gem:*` release
  steps until then.
- **Untested release matrix:** oxtree (oxc git dependencies, fat LTO) has
  only been built for aarch64-apple-darwin. It has never been compiled for
  x86_64-darwin, linux-gnu or musl. Run the workflow on a test tag first.

## Left

- Adapt `gems/oxtree` once tree-cli's `cli.rs` lands, if the executable
  name or flags need it. Today it's pure pass-through, and the binary only has
  the dev commands (`--format-file` and so on).
- Land the CI commit (`tree-pkg-wip`, 0d31dcd) once main's compare gate is
  green.
- `rake vendor:rubocop` (runtime's) uses `../rubocop` relative to the
  checkout, so it fails from a worktree. `oxtree:vendor`'s `oxtree_sibling`
  shows the fix if runtime wants it.
