# tree-cli: the oxtree CLI, stree_new mode, compare_with_stree

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

oxtree stree-new check|write [--modified] [--print-width=N] [--source-files=GLOB]... [--oxfmt=PATH]
oxtree stree-new check:modified  # same as check --modified (also write:modified; stree_new is an alias)
```

- `rake stree_new:check` → `oxtree stree-new check`; `stree_new:write:modified`
  → `oxtree stree-new write --modified`. `FILES="a b"` works as in the rake task.
- stree-new defaults are code/'s: width 100 and the globs
  `**/*.rb Rakefile **/*.rake Gemfile **/*.html.erb bin/generate-openapi-spec
  **/*.{js,css,json}.erb` (written as three globs). `--source-files` replaces
  the whole list.
- stree-new runs relative to `git rev-parse --show-toplevel` (else the cwd):
  `.streerc`, `.oxfmtrc.json`, `node_modules/.bin/oxfmt` and printed paths.
- oxfmt executable: `--oxfmt=PATH`, else `$OXFMT_BIN`, else
  `<root>/node_modules/.bin/oxfmt`. It's only needed for the few inputs the
  linked-in oxc formatter can't do.
- Plugins use upstream's names: `plugin/single_quotes`, `plugin/trailing_comma`,
  `plugin/disable_auto_ternary`, `erb`. Anything else is a LoadError, exit 1.
- Hidden flags: `stree-new --list` (selected files), `stree-new --emit-json`
  (formatted output per file, for compare_with_stree),
  `stree-new --oxfmt-backend=process`; the old dev commands
  `--format-list`, `--format-embedded-list`, `--format-erb-list`,
  `--format-erb`, `--format-file`, `--dump-ast` still work.
- Exit codes: 0 success, 1 any failure (unformatted, parse error, bad
  option, unsupported command).

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
- `cli/stree_new.rs`: port of `stree_new.rake`.
- `cli/tests.rs`: the applicable cases of syntax_tree's `test/cli_test.rb`
  plus end-to-end stree-new tests in a temp git repo. `cargo test -p oxtree --lib`.
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
- stree-new: a missing `.streerc` means no ignores (rake raises); equal-size
  files keep git order.
- Tailwind (user decision): when an input needs Tailwind class sorting and
  the oxfmt executable is missing, the library's unsorted output is used
  and `[warn] <file>: oxfmt not found; Tailwind classes left unsorted` goes
  to stderr; it doesn't count as a failure. A present oxfmt that rejects
  the code still leaves it verbatim, as the rake task does. The executable
  backend (`--oxfmt-backend=process`, or `--no-default-features`) still
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
selection, every file's output (or error) and the `stree-new check` report,
in memory. code/ needs no node_modules; nothing is written to it (checked
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
come up. `--modified` selection also matches: `StreeNew.modified_files` and
`oxtree stree-new --list --modified` give the same 508 files on code/'s
`rubocop-rs` branch (checked by hand; the script compares `all_files`).

Timing (`--time 3`, fastest of 3, release build, machine load average ~25
on 14 cores from the other agents, so absolute numbers are inflated):

| Command | Time |
| --- | --- |
| `oxtree stree-new check` | **0.72 s** (prints 627 ms excluding git discovery) |
| stree_new check in code's bundle, with oxfmt (complete) | 9.54 s |
| `bin/rails stree_new:check` as is (no node_modules, so ERB embeds error out: incomplete) | 11.17 s (earlier baseline: 11.6 s) |

So oxtree is about 13x faster than the complete stree_new run and 15x
faster than the Rails task.

## Left

- The unported stree actions (`ast`, `doc`, `expr`, `json`, `match`,
  `search`, `ctags`, `lsp`). Nothing in code/ uses them.
- Optional: syntax-color the parse-error source line on a TTY, like IRB.
