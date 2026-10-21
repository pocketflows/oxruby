# oxtree: status and handoff

oxtree is a Rust port of syntax_tree 6.3.0 (the Ruby formatter), its ERB
plugin w_syntax_tree-erb 0.12.0, and `../code`'s `ErbEmbeddedFormatter`
(oxfmt for `<script>`/`<style>` bodies and `.js/.css/.json.erb` files). The
goal is a drop-in replacement for `bin/rails stree_new:{check,write}` in
`../code`, shipped as the `oxtree` gem. All work is on branch `stree`.

## What's done

| Piece | Status | Evidence |
| --- | --- | --- |
| prettier_print 1.2.1 printer | Done | `crates/oxtree/src/prettier_print.rs` |
| syntax_tree tree, built from Prism | Done | `parser.rs` (port of parser.rb), `node.rs` |
| syntax_tree formatting | Done | `format.rs`, `format_nodes.rs` (port of node.rb) |
| syntax_tree fixtures | **789/789 pass** | `cargo test -p oxtree --test fixtures` |
| Idempotency of fixture outputs | **789/789** | same test file |
| ERB plugin | Done | `erb/parser.rs`, `erb/format.rs` |
| ERB fixtures (`test/fixture/*`) | **7/7 pass** | not yet in `cargo test`, see "What's left" |
| `ErbEmbeddedFormatter` port | Done | `embedded.rs` |
| oxfmt: oxc crates linked in | Done | `oxc.rs`, `oxfmt.rs`, feature `oxc` (default) |
| CLI (`stree` drop-in, stree_new) | **Not started** | `main.rs` only has dev commands |
| Gem `gems/oxtree`, `rakelib/oxtree.rake` | **Not started** | |
| Differential script in `script/` | Partial | Ruby reference scripts in `crates/oxtree/tools/` |

### Parity on real code (all byte-for-byte)

Reference outputs came from the real gems on Ruby 3.3.11. The scripts are
in `crates/oxtree/tools/`; each `reference_*.rb` header names the matching
`oxtree` dev command.

| Corpus | Files | Matching | Width |
| --- | --- | --- | --- |
| `../code` Ruby (`**/*.rb`, Rakefile, `*.rake`, Gemfile, bin/generate-openapi-spec, minus .streerc ignores) | 3,932 | **3,932** | 100 |
| Ruby 3.3 stdlib (upstream's idempotency corpus) | 997 | **997** | 80 |
| Installed gems' `lib/**/*.rb` (latest version of each gem) | 21,936 | **21,936 formatted files**; 27 files that fail to parse differ only in the error message text (Prism's vs Ripper's) | 80 |
| `../code` `**/*.erb`, plain `SyntaxTree::ERB.format` | 651 | **651**, including the 12 ERB parse errors (message, line and column) | 100 |
| `../code` `.html/.js/.css/.json.erb` through `ErbEmbeddedFormatter` + real oxfmt 0.72.0 | 636 | **636** with the linked-in oxc formatter, and 636 with the oxfmt-executable backend | 100 |

### Speed (`../code`, check mode, single-threaded dev commands)

These dev commands format without git discovery and without parallelism,
so this isn't yet the `stree_new:check` comparison. That needs the CLI.

| Run | Time |
| --- | --- |
| baseline `stree_new:check`, as reported (incomplete: oxfmt missing) | 11.6 s wall |
| real syntax_tree, 3,932 Ruby files, one process | 16.8 s |
| oxtree, same files (release build) | 1.8 s, including writing 3,932 output files |
| real ERB + ErbEmbeddedFormatter + oxfmt subprocesses, 636 files | 22.7 s |
| oxtree, same files, oxc linked in | 0.4 s |
| oxtree, same files, oxfmt subprocesses | 4.1 s |

## How it works (read before changing anything)

- **Prism, not Ripper.** syntax_tree builds its tree from Ripper events and
  derives node locations from the tokens each event consumes. Formatting
  depends on those locations: blank lines between statements come from
  `start_line`/`end_line`, and comment placement from char ranges.
  `parser.rs` rebuilds syntax_tree's exact node shapes from Prism, using
  `prism/translation/ripper.rb` as the Prism-to-Ripper map, and computes
  locations with the same formulas as parser.rb.
  - Heredocs extend `end_line` through `Location#to`.
  - `Statements#bind` uses `find_next_statement_start` bounds.
  - Leading `void_stmt`s are reproduced.
- **Comments** are attached in two phases as upstream does:
  1. `Statements#bind` runs bottom-up in Ripper's reduction order. `case`
     clauses are visited in source order but reduced last-to-first.
  2. `attach_comments`/`nearest_nodes` use `child_nodes` order.
- **Ripper token bookkeeping that leaks into output** is emulated, because
  the corpora showed it matters:
  - `when` grabs the last unconsumed `then` keyword anywhere before it.
  - `a**b` (no space) leaves a `**` token that a later hash pattern without
    a rest steals, which prints `**`.
  - `&blk` after a heredoc argument is dropped.
  - `in pattern if guard` is a modifier if.
  - Lexer positions are approximated for empty `Args`/void statements.

  Look for `then_tokens`/`pow_tokens` in `parser.rs`.
- **Frozen strings.** prettier_print's array-buffer `trim!` only strips
  unfrozen strings. `text(&'static str)` stands for a Ruby literal (frozen)
  and `text_owned(String)` for a computed string.
- **Debugging a mismatch.** Diff the trees first:
  - reference: `ruby crates/oxtree/tools/dump_ast.rb FILE`, which needs the
    syntax_tree gem (e.g. `RBENV_VERSION=3.3.11`);
  - oxtree: `target/debug/oxtree --dump-ast FILE`.

  On the fixtures, 771/789 trees are identical. The rest differ only in
  lexer-position details that don't affect output.
- **ERB.** The parser's `maybe` doesn't rewind the token stream, and the
  tokenizer raises only when the parser reaches the bad token. Both
  behaviors are kept on purpose.
- **oxfmt.** The library backend (`oxc.rs`) is used when `.oxfmtrc.json`
  only sets core options (`printWidth`, `tabWidth`, `useTabs`, `endOfLine`),
  Tailwind keys, `$schema` and `ignorePatterns`. The executable is used
  instead when:
  - the config has any other key, or there's an `.editorconfig` or a `.ts`
    config: `Config::load` returns `None`;
  - an input reaches the Tailwind sorter or an embedded-language template.

  The executable defaults to `<root>/node_modules/.bin/oxfmt`, as the rake
  task does, and a missing executable is an `OxfmtMissingError`.

## Build notes

- `ruby-prism = "=2.0.0-rc.2"` is pinned in the root `Cargo.toml`, as on
  the oxcop branch: ruby-prism-sys links the C library, so the workspace can
  only hold one version. **`crates/oxcop` on this branch still uses the 1.9
  API, so workspace-wide `cargo build` fails until `stree` is merged with
  `oxcop-cops`.**
- Until then, check oxtree with:

  ```sh
  cargo build -p oxtree
  cargo test -p oxtree
  cargo clippy -p oxtree --all-targets -- -D warnings
  ```
- The root `Cargo.toml` has `[patch.crates-io] oxc_allocator = { git = …,
  tag = "oxfmt_v0.72.0" }`. oxc's workspace does the same.
- oxtree sets `rust-version = "1.97"`, the oxc crates' MSRV.
- The first build fetches the oxc repository, about 200 MB.
- `cargo build --no-default-features` drops oxc and always runs oxfmt.
- `#![forbid(unsafe_code)]` is set in `lib.rs` and `main.rs`.

## What's left, in priority order

1. **CLI** (`src/cli.rs`, replacing the dev-only `main.rs`).
   - Port syntax_tree's `cli.rb`, reading it in full: `check`, `write`,
     `format`, `--print-width`, `--plugins` (single_quotes, trailing_comma,
     disable_auto_ternary map to `format::Options`; erb), `.streerc`
     parsing, `--ignore-files` with plain `File.fnmatch` semantics (no
     `FNM_PATHNAME`), and stree's exact messages and exit codes. Check
     against `vendor/syntax_tree/test/cli_test.rb`.
   - Add the stree_new mode as a subcommand, for example
     `oxtree stree-new check|write [--modified]`:
     - git file discovery: `git ls-files -z` plus untracked files;
     - the source globs with `FNM_PATHNAME|FNM_EXTGLOB`;
     - `FILES=`;
     - `--modified`, which diffs against `git merge-base main HEAD`;
     - largest-first ordering and rayon parallelism;
     - output lines `[warn] file` / `[error] file: Class: msg`, then the
       summary line and exit 1, as in `code/lib/tasks/stree_new.rake`.
   - Handler choice:
     - `.erb` files go to `embedded::format`, using the file path;
     - everything else goes to the Ruby formatter at `print_width` with
       default options (stree_new loads no Ruby plugins).
   - Read files like `SyntaxTree.read`, honoring magic encoding comments.
     UTF-8 is enough for code/.
2. **Gem and rake tasks.**
   - `gems/oxtree`: copy `gems/oxcop`'s launcher, gemspec and
     `exe/<platform>/` layout.
   - Probably also ship an `stree`-named executable, off by default so it
     doesn't clobber syntax_tree's.
   - `rakelib/oxtree.rake`: namespace everything under `oxtree:`, because
     Rake merges same-named tasks with oxcop's `gem:*`/`build`. Include a
     `vendor` task that re-syncs `vendor/syntax_tree*`.
3. **Differential script** `script/compare_with_stree`.
   - Run real syntax_tree from code/'s bundle, undoing rbenv's pins as
     `script/compare_with_rubocop` does, and oxtree in check mode over the
     same file list.
   - Compare outputs in memory and never write to code/.
   - Point `ErbEmbeddedFormatter::OXFMT_BIN` at an oxfmt installed outside
     code/; see `tools/reference_embedded.rb`.
   - Then time `oxtree stree-new check` against `bin/rails stree_new:check`.
4. **Wire the ERB suite into tests.**
   - Add a Rust test that formats `vendor/syntax_tree-erb/test/fixture/*`
     (7/7 pass today by hand) and checks idempotency.
   - Port the string cases in `erb_test.rb`/`html_test.rb`: the
     `assert_formatting` pairs and the parse-error line, column and message
     assertions. The tests that assert Ruby node classes can't apply to a
     binary; list them as skipped.
   - Also port the plugin tests in `vendor/syntax_tree/test/plugin/` (quote,
     trailing comma and ternary options). They're implemented but not yet
     covered by tests.
5. Add `bundle exec rake` integration (an `oxtree:check` task) and an
   additive CI line.
6. Port the remaining fidelity edge cases if real code hits them:
   - leftover `^` tokens turning a later `begin` into `PinnedBegin`;
   - `break`/`next` arguments with a `do` block;
   - nested `MLHSParen` locations;
   - lambda-local line numbers.

## Known gaps and open decisions

- **Parse errors.** Prism and Ripper disagree on error *messages*, and very
  rarely on what counts as an error. Output for valid code is unaffected.
  The `[error] file: SyntaxTree::Parser::ParseError: <msg>` lines will name
  the same files but with different message text. Decide whether that's
  acceptable.
- **Target Ruby.** Prism 2.0 parses with its default version, not
  `TargetRubyVersion`. syntax_tree on code/ runs on Ruby 3.3's Ripper. No
  difference was seen on any corpus. `parse_with_options` could pin 3.3
  later.
- **Tailwind sorting.** The pure-Rust path never sorts Tailwind classes; it
  hands those inputs to the oxfmt executable. On code/ no embedded snippet
  reaches the sorter. If one ever does and node_modules isn't installed,
  oxtree errors as the rake task would. Decide whether a missing oxfmt
  should instead fall back to the unsorted library output with a warning.
- **`stree` executable name.** Shipping one from the oxtree gem would shadow
  syntax_tree's when both are bundled. The suggestion is to opt in via an
  env var or a separate `oxtree-stree` gem.
- **Dropped `&blk`.** syntax_tree 6.3.0 drops `&blk` after a heredoc
  argument, and `a**b` followed by a hash pattern inserts `**`. Both change
  the program, and oxtree reproduces both for byte-parity. Decide whether to
  keep that parity or deliberately diverge and fix them.
