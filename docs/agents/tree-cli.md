# tree-cli: the oxtree CLI, ERB like stree_new, compare_with_stree

## Interface (for tree-pkg and the gem)

One executable, **`oxtree`** (`crates/oxtree/src/main.rs` → `cli::run`).
No `stree`-named executable or alias (user decision). Arguments pass
through verbatim; the gem launcher should `exec` the platform binary with
`ARGV` unchanged.

```text
oxtree check|format|write|debug [--plugins=...] [--print-width=N] [-e SCRIPT] [--extension=EXT]
                                [--target-ruby-version=V] [--ignore-files=GLOB]... [--config=FILE] FILE...
oxtree help | version            # version prints 6.3.0 (SyntaxTree::VERSION), as stree does
oxtree --version                 # oxtree 0.1.0 (syntax_tree 6.3.0, w_syntax_tree-erb 0.12.0)

oxtree [-w|--write|-c|--check|-l|--list-different] [options] [PATH...]   # flags.rs
```

- `rake stree_new:check` → `oxtree --check .`; `stree_new:write` →
  `oxtree --write .` (or `oxtree .`). The `stree-new` subcommand was removed
  (2026-10-22, user decision), with `FILES=` and `--modified`: name the
  files as PATHs instead.
- A directory PATH expands to its Ruby files (`*.rb`, `*.rake`,
  `*.gemspec`, `*.ru`, `Gemfile`, `Rakefile`) and stree_new's ERB globs
  (`*.html.erb`, `*.js.erb`, `*.css.erb`, `*.json.erb`; not `.text.erb`),
  from git (tracked plus untracked, not ignored), without dot-directories,
  whatever `--plugins` says. Every `.erb` file the flag CLI formats goes
  through `embedded::format` (code/'s `ErbEmbeddedFormatter`); a named
  `.text.erb` is an `UnsupportedFileError`. `--plugins=erb` is accepted and
  changes nothing there. stree's commands keep syntax_tree's ERB handling.
- oxfmt: `.oxfmtrc.json` and the executable are looked up in `git
  rev-parse --show-toplevel` (else the cwd); the executable is `$OXFMT_BIN`,
  else `<root>/node_modules/.bin/oxfmt`. It's only needed for the few inputs
  the linked-in oxc formatter can't do (Tailwind sorting among them).
- Files are formatted largest first on rayon; output stays in input order.
- Known selection differences from stree_new (by design, reported by
  compare_with_stree): extensionless files stree_new names by path
  (`bin/generate-openapi-spec`) aren't found by a directory walk; `*.ru`,
  `*.gemspec`, nested `Gemfile`/`Rakefile` and dotfiles outside
  dot-directories are added. On code/ that's `bin/generate-openapi-spec`
  vs `config.ru` and `cable/config.ru`.
- Plugins use upstream's names: `plugin/single_quotes`, `plugin/trailing_comma`,
  `plugin/disable_auto_ternary`, `erb`. Anything else is a LoadError, exit 1.
- Hidden flags: `oxtree --list-files PATH...` (selected files),
  `oxtree --emit-json PATH...` (formatted output, or `ERROR Class:
  message`, per file, for compare_with_stree); the old dev commands
  `--format-list`, `--format-embedded-list`, `--format-erb-list`,
  `--format-erb`, `--format-file`, `--dump-ast` still work.
- Exit codes: stree's commands 0 success, 1 any failure (unformatted, parse
  error, bad option, unsupported command); the flag CLI 0/1/2 (formatted,
  not formatted, error).

## Done

- `cli/mod.rs`: port of cli.rb. `check`, `debug`, `format`, `write`, `help`,
  `version`; OptionParser semantics (options anywhere, unique-prefix
  abbreviations, `--opt=v` and `--opt v`, Ruby Integer parsing, the
  invalid/ambiguous/missing-argument errors); `.streerc` and `--config`;
  `--ignore-files` with quote stripping and plain `File.fnmatch`; stdin when
  no files and no `-e`; stree's exact messages, ANSI colors (always on, as
  upstream), parse-error highlight and exit codes. Files are read like
  `SyntaxTree.read`.
- `cli/fnmatch.rs`, `cli/glob.rs`: ports of Ruby's `File.fnmatch` (with
  `FNM_PATHNAME`/`FNM_EXTGLOB`/brace expansion) and `Dir.glob` (sorted,
  depth-first, no hidden entries, `**/` doesn't follow symlinks), checked
  against Ruby 3.3 on oracle tables.
- `cli/flags.rs`: the flag CLI, with ERB formatted like `stree_new.rake`
  (which `cli/stree_new.rs` ported until 2026-10-22).
- `cli/tests.rs`: the applicable cases of syntax_tree's `test/cli_test.rb`
  plus end-to-end flag-CLI tests (ERB without flags, `.text.erb` skipped,
  the Tailwind warning, check/write/-l, `--emit-json`), some in a temp git
  repo. `cargo test -p oxtree --lib`.
- Shared-code edits (small, additive): `erb::format_with_options` so Ruby in
  ERB tags honours `--plugins` like upstream; `oxc::Outcome::NeedsTailwindSort`
  and `oxfmt::take_tailwind_unsorted` for the Tailwind fallback; comments
  at the two kept syntax_tree bugs in `parser.rs`.

### cli_test.rb parity

Ported and passing: check, check_unformatted, check_print_width,
check_target_ruby_version, debug, format, write, write_script, write_stdin,
help, help_default, no_arguments, inline_script, multiple_inline_scripts,
version, config_file, print_width_args_with_config_file(_override),
plugin_args_with_config_file, config_file_custom_path(_space_separated),
config_file_nonexistent_path, and plugins (with syntax_tree's own plugins).

- `test_write_syntax_tree` passes except the `"syntax error"` substring:
  messages are Prism's (user decision), e.g. `unexpected '<', ignoring it`
  where Ripper says `syntax error, unexpected '<'`. Line, column and the
  highlight are byte-identical.
- Not applicable to a binary: test_handler, format_*_with_custom_handler
  (Ruby handler objects), test_plugins/test_config_file's arbitrary Ruby
  plugin files, debug_non_idempotent_format and generic_error (stub
  `SyntaxTree.format`).
- Not ported (print Ruby objects, or the LSP): ast, ast_ignore,
  ast_syntax_error, doc, expr, expr_more_than_one, json, match, search*,
  language_server, ctags. They exit 1 with "`<name>` is not supported".

### Differences from upstream (cosmetic)

- Output is printed in input order; upstream's thread order varies.
- The highlighted source line isn't IRB-colorized (upstream colors it only
  on a TTY).
- Uncaught Ruby exceptions print `oxtree: <message> (<Class>)`, no backtrace.
- Help text says `oxtree` where upstream says `stree`.
- On case-insensitive file systems Ruby's `Dir.glob` rewrites a literal last
  component to its on-disk case; oxtree keeps it as typed.
- ERB via the flag CLI vs stree_new: the report is stree's (`[warn] file`
  on stderr, parse-error highlight) rather than stree_new's `[error] file:
  Class: msg` lines and summary; a missing `.streerc` means no ignores (rake
  raises); `.streerc` plugins apply to Ruby files (stree_new ignores them;
  code/ has none); ignores also skip directories.
- Tailwind (user decision): when an input needs Tailwind class sorting and
  the oxfmt executable is missing, the library's unsorted output is used
  and `[warn] <file>: oxfmt not found; Tailwind classes left unsorted` goes
  to stderr; it doesn't count as a failure. A present oxfmt that rejects
  the code still leaves it verbatim, as the rake task does. The executable
  backend (`--no-default-features`) still
  errors with OxfmtMissingError. An oxfmt that exists but can't be run is
  still an error (tested); its message still says "not found; run `npm ci`"
  because `run_process` has one error for every spawn failure (the rake task
  would raise `Errno::EACCES` there). A one-line fix if anyone cares.

Note: `.streerc`'s `--ignore-files=db/**/*.rb` uses plain fnmatch, so it
needs a `/` after `db/x`: `db/schema.rb` and `db/seeds.rb` are *not*
ignored by stree or stree_new. oxtree matches that.

## compare_with_stree and timing

`OXFMT_BIN=<oxfmt 0.72.0> script/compare_with_stree [PROJECT] [--print-width N] [--time [RUNS]]`
(needs `cargo build --release -p oxtree`, or `OXTREE_BINARY`). It loads the
project's `stree_new.rake` in the project's bundle without booting Rails,
swaps `ErbEmbeddedFormatter::OXFMT_BIN` for `OXFMT_BIN`, and compares file
selection (`oxtree --list-files .`), every file's output or error
(`oxtree --emit-json .`) and the files `oxtree --check .` reports, in
memory. The known selection differences above are printed but don't fail
it. code/ needs no node_modules; nothing is written to it (checked
with `find -newer` after each run). oxfmt 0.72.0 was installed earlier in a
session scratchpad (`.../scratchpad/oxfmt-research/npm/node_modules/.bin/oxfmt`);
that path is ephemeral, so `npm i oxfmt@0.72.0` anywhere outside code/.

Results on code/ (9bf6841d2, 4,515 files: 3,933 Ruby-handler files, 582 ERB):

| Width | Files selected (both) | Outputs identical | Check report |
| --- | --- | --- | --- |
| 100 (code's) | 4,515 | 4,515 | identical (0 lines; code/ is formatted) |
| 80 | 4,515 | 4,515 | identical (2,880 `[warn]` lines) |
| 40 | 4,515 | 4,515 | identical (4,013 `[warn]` lines) |

No parse errors on code/, so the Prism-vs-Ripper message difference didn't
come up. (`--modified` was checked against `StreeNew.modified_files` by
hand, before it was removed with the subcommand.)

The numbers above were taken with `oxtree stree-new`. After the move to the
flag CLI (2026-10-22), on code/ 4d8b7c73b, a benchmark branch with
formatting faults injected into ERB files with JS and CSS:
`script/compare_with_stree` passes: stree_new 4,525 files, oxtree 4,526,
4,524 in common with identical output; known differences
`bin/generate-openapi-spec` (stree_new only, formatted) and `config.ru`,
`cable/config.ru` (oxtree only, formatted); `oxtree --check .` reports the
same 9 files as stree_new's check (4 Ruby, `_chartkick.html.erb`,
`_meta_pixel.js.erb`, `brand_kit.html.erb`, `gtm_snippet.html.erb`,
`_markdown.html.erb`), exit 1. `--write .` on a copy of code/ writes exactly
stree_new's output for those 9, then `--check .` passes.

| Command (code/, best of 4, release) | Time |
| --- | --- |
| `oxtree --check .` | **0.52 s** |
| old `oxtree stree-new check` | 0.53 s |

## Left

- The unported stree actions (`ast`, `doc`, `expr`, `json`, `match`,
  `search`, `ctags`, `lsp`). Nothing in code/ uses them.
- Optional: syntax-color the parse-error source line on a TTY, like IRB.
