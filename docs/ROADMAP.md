# Roadmap

Status as of 2026-10-21, after the first parallel phase (see
`docs/PARALLEL.md`). Per-agent detail lives in `docs/agents/<name>.md`; this
file is the consolidated view. oxtree has its own status file,
`docs/OXTREE.md`.

## oxcop: where it stands

- **Cops:** all 613 of RuboCop 1.91.0's core cops are ported. Each passes
  its whole vendored upstream spec through the binary: 29,116 examples,
  0 failures, 5 pending.
- **Plugin cops:** the five that ../code enables, each passing its whole
  vendored upstream spec: Rails/TimeZone, Rails/TimeZoneAssignment and
  Rails/HttpStatusNameConsistency (rubocop-rails 2.37.0), and
  RSpec/EmptyLineAfterHook and RSpec/EmptyLineAfterExample (rubocop-rspec
  3.10.2), on a port of rubocop-rspec's `Language` and `EmptyLineSeparation`.
  They run only when the project loads the plugin. Specs come from
  `rake vendor:plugins` (`docs/agents/rails.md`, `docs/agents/rspec.md`).
  Not yet checked: user additions under `RSpec: Language:` merging with the
  defaults (`inherit_mode`).
- **App cops (compiled-in plugins):** an application ports its own cops to
  Rust and builds its own `oxcop` binary on the library
  (`oxcop::Oxcop::new().plugin(..).run()`, `docs/PLUGINS.md`). Plugin cops
  join the registry, their default.yml is integrated like a lint_roller
  plugin's, and they claim the `require:` entries they replace.
  `script/plugin_specs` runs the app's own cop specs against its binary.
- **Project index:** `AllCops/UseProjectIndex` and `ProjectIndexIncludesGems`
  work on Shopify's `rubydex` crate (pinned git dependency), and every
  `:project_index` example passes except two that call the Ruby cop's
  private cache directly (`docs/agents/index.md`). On ../code the index adds
  about 0.7 s, or 3.4 s with gems.
- **Real code:** `script/compare_with_rubocop ../code` runs every ported cop
  with `--only`, plus the app's own config with plugin defaults. oxcop and
  real RuboCop inspect the same 5,692 files and report identical offenses.
  Timing with a debug build: rubocop 91 s, oxcop 20 s.
- **Tokens:** `processed_source.tokens` is rebuilt in safe Rust from Prism's
  tree. It matches `Prism.lex` and RuboCop's tokens with 0 differences on
  ../code, rubocop's `lib/` and `spec/`, and about 6,500 gem files
  (`script/compare_ast --tokens` / `--prism-tokens`).
- **No `unsafe`:** every crate forbids it. Prism comes from crates.io
  `ruby-prism =2.0.0-rc.2`. The regexp_parser gem is ported to Rust and
  matches the gem on 12,867 real and 40,000 fuzzed regexps.
- **Config:**
  - `inherit_from`, `inherit_mode`, `inherit_gem`, and ERB in
    `.rubocop.yml` (rendered by Ruby).
  - Plugin default configs, through the launcher's `OXCOP_GEMS`.
  - Preview, pending cops (the warning, `--enable-pending-cops`, `NewCops`)
    and `--only` force-enabling.
- **Runner:**
  - Parallel inspection (rayon, on by default).
  - The autocorrect loop.
  - Comment directives, including push/pop and `*-next`.
  - The directive cops: RedundantCopDisableDirective and its siblings.
  - `--fail-level`, `--display-only-*`, `-F` and
    `--ignore-disable-comments`.
  - Formatters: progress, simple, quiet, clang, emacs, json and offenses
    (without its terminal progress bar).
- **Specs:** one `oxcop --server` per rspec process. `script/parallel_rspec`
  runs the suite in about 32 s, and `script/land` uses it.

### Left, in priority order

1. ~~**The three project-index cops.**~~ Done (`docs/agents/index.md`).
2. ~~**Invalid UTF-8 without a magic comment.**~~ Done: it fails with
   Lint/Syntax as in RuboCop (`docs/agents/utf8.md`).
3. **Remaining CLI options:** `-o/--out`, `--auto-gen-config` (with
   `--exclude-limit` and friends), `-x`, `--safe`, `--force-default-config`,
   `--show-cops`, `--show-docs-url`, `-E/-S/--extra-details`,
   `--display-time`, `--editor-mode`, `-V`, `--require`/`--plugin`, plus
   `AllCops: DefaultFormatter`. `docs/agents/runtime.md` has the full list.
4. **Result cache:** `--cache` is accepted and ignored today.
5. **ConfigValidator:** warnings for unknown cops and parameters, and
   rejecting invalid values.
6. **Config gaps:**
   - globs and remote URLs in `inherit_from`;
   - `handle_disabled_by_default`, `add_missing_namespaces`,
     `add_excludes_from_files`;
   - ConfigFinder's project-root stop;
   - plugins whose rules aren't `config/default.yml`.
7. **Remaining TargetRuby sources:** `Gemfile.lock`, `*.gemspec` and
   `RUBOCOP_TARGET_RUBY_VERSION`.
8. **Release:** the release workflow builds both gems for six platforms, but
   only aarch64-apple-darwin has ever been built. Run it on a test tag first.

### Known gaps and approximations

- **Parsing:** always Prism, at `TargetRubyVersion`. RuboCop uses the
  `parser` gem below 3.3, so the consequences are:
  - `Lint/Syntax` messages are Prism's;
  - below 3.3, Prism's "Invalid retry without rescue" is dropped to match
    `parser`;
  - squiggly-heredoc `\\` unescaping follows Prism (8 files in rubocop's own
    specs).
- **The project index:** the gem crashes on a `module_function` method's
  visibility; oxcop treats it as private. With `ProjectIndexIncludesGems`,
  only gems' `lib/**/*.rb` is indexed, as upstream, so constants defined in a
  gem's `app/` are reported by NameTypo.
- **Lint/ScriptPermission** reads `executable?` as the owner's execute bit,
  since the effective uid isn't available without FFI.
- **LineLengthHelp `valid_uri?`** treats a `mailto:` URI that upstream would
  crash on as not a URI.
- **NameSimilarity** suggestions rank the sorted cop-name list. RuboCop uses
  registration order, which only matters on an exact tie.
- **Slow cop:** Layout/RedundantLineBreak is left out of the ../code compare
  (`COMPARE_SLOW=1` includes it), because real RuboCop takes about 540 s on
  it.
- **Spec-only support:** stubbed `Dir.pwd` needs the spec server; it doesn't
  work with `OXCOP_NO_SERVER`.

## oxtree

The formatter, the `oxtree` CLI and the gem are done. `oxtree --check .`
replaces code/'s `bin/rails stree_new:check`, ERB included, with no flag:
it reports the same files with byte-identical output (4,524 files both
select; `script/compare_with_stree`), in 0.52 s against 11.2 s. The
`stree-new` subcommand is gone. `docs/OXTREE.md` covers the details and what's left: the stree
actions that print Ruby objects, and the untested release matrix.
