# config: notes

Owner: `config` (round 2). Scope: configuration loading gaps (ROADMAP
"Left" items 6 and 7).

## Done

- **Real-config gate.** `REAL_CONFIG=1 script/compare_with_rubocop` runs
  both tools with the project's own configuration (no `--only`) and
  compares:
  - offenses of ported cops only (plugin and custom cops only run in
    RuboCop). Lint/RedundantCopDisableDirective offenses that name an
    unported cop, a department or "all cops" are dropped on both sides,
    since their verdict depends on cops oxcop doesn't run;
  - each ported cop's resolved configuration: whether it runs
    (`Registry#enabled_cop_name?`) and its `for_cop` parameters. RuboCop's
    side is a small Ruby script in the project's bundle; oxcop's is the
    hidden `--show-resolved-cops` (JSON; non-finite floats as strings,
    `!ruby/regexp` and `:symbol` scalars as their source text, which the
    Ruby side mimics).
  `script/land` runs it after the `--only` compare on full gates.
  Why both checks: ../code has only 4 offenses under its own config, so an
  offense diff alone could not see the DisabledByDefault bug (Style/HashSyntax
  and Style/NumberedParameters have no violations there). Before the fix the
  config check reported exactly those two cops (rubocop enabled, oxcop not);
  after it, 0 differences and 0 mismatched files.
- **`handle_disabled_by_default`** and the rest of `merge_with_default`
  ported faithfully: the `Enabled` transform covers every top-level entry
  (AllCops and departments too), user-config entries get `Enabled: true`
  unless set, enabled departments restore their cops' default `Enabled`.
  Also `--disable-all-cops`, `--enable-all-cops` (and their
  OptionArgumentError), `--[no-]preview` (`ConfigLoader.preview`,
  `Config#preview?(options)`), `apply_default_overrides` (no config file),
  `with_preview_exclude_merge`, and `enable_cop?`'s nested-department
  lookup. `ConfigStore::from_options` sets all of it from `Options`.
  Tests: the DisabledByDefault / EnabledByDefault examples of
  `config_loader_spec.rb`, as unit tests in `config_loader.rs`.
- **TargetRuby** (`target_ruby.rs`): every upstream source, in order:
  `RUBOCOP_TARGET_RUBY_VERSION`, `TargetRubyVersion`, the gemspec's
  `required_ruby_version` (oldest known Ruby satisfying it), `.ruby-version`,
  `mise.toml`, `.tool-versions` (all searched upwards, as `FileFinder`
  does), the lockfile's `RUBY VERSION`, default. Resolved once per `Config`
  (`Config::target_ruby()`). Unit tests port `target_ruby_spec.rb`'s cases.
- `file_finder.rs` (`FileFinder`), `gem_version.rs` (RubyGems'
  `Gem::Version` / `Gem::Requirement`, for the gemspec source and for
  `requires_gem` later), `Config::bundler_lock_file_path`.

- **ConfigFinder** stops at the project root (topmost `Gemfile` /
  `gems.rb` above the working directory, cached per run in `ConfigLoader`)
  and then tries `<root>/.config/.rubocop.yml` and
  `<root>/.config/rubocop/config.yml` before the user's files.
- **`add_excludes_from_files`**: the topmost `.rubocop.yml` up to the
  project root adds its `AllCops: Exclude` to nested configurations;
  `--ignore-parent-exclusion` (new) turns it off.
- **`add_missing_namespaces`**: unqualified or wrongly qualified cop names
  are qualified, with upstream's "no department given" / "has the wrong
  namespace" warnings and the AmbiguousCopName error. The registry is core
  cops + plugin cops from the default configuration + the file's own keys
  outside core departments (plugin/custom cops, which upstream has
  registered by then). Not ported: skipping ConfigObsoletion's deprecated
  names (ConfigObsoletion itself isn't ported).
- **`inherit_from` globs**, through `path_util::dir_glob` (a `Dir.glob`
  port: braces, `**/`, no dotfiles, relative to the working directory as
  upstream's is).
- **`gem_versions_in_target`** in `lockfile.rs` (index's module, extended):
  `TargetGems::versions` / `version(name)`, `cx.target_gem_version(name)`,
  and `Cop::gem_requirements()` (`requires_gem`), checked by the team as
  `target_satisfies_all_gem_version_requirements?` does. No core cop uses
  it; plugin ports will. The spec shim's `--target-gems` accepts an
  optional `versions` map but the shim doesn't send it yet.

All checked against rubocop (code's bundle, `BUNDLE_GEMFILE`) on scratch
projects, and gated by the real-config compare on ../code (0 differences).

## Left

- Remote `inherit_from` URLs: needs an HTTP client and RemoteConfig's
  cache (`cache_root`, 24 h). **Decision for the user:** add a crates.io
  HTTP dependency, shell out to `curl`, or let the Ruby launcher fetch them.
- Plugins whose `rules` aren't `config/default.yml` (`type: :object`):
  needs the plugin's Ruby code; the launcher could resolve them and pass
  the hash.
- `XDG_CONFIG_HOME` containing `~` isn't expanded (`File.expand_path`
  would).
- ConfigValidator (roadmap item 5) would use `target_ruby::is_supported`
  and `Source::name` for the "unsupported Ruby version" error.
