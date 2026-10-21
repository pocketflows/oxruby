# Roadmap

## oxcop: drop-in compatibility gaps

Found so far, roughly in the order they block real projects (`../code`
needs most of these):

- [ ] **Plugin default configs.** Plugins can't run, but their
      `config/default.yml` still changes core behavior. For example,
      rubocop-rails adds `AllCops: Exclude: bin/*`, which is the only remaining
      diff on `../code`. The Ruby launcher can resolve plugin gem paths and
      pass them to the binary.
- [ ] `inherit_mode` (merge/override), `inherit_gem`, remote `inherit_from`.
- [ ] ERB in `.rubocop.yml` (`../code` computes `TargetRubyVersion` with ERB).
- [ ] Comment directives (`# rubocop:disable`, `:enable`, `:todo`,
      `rubocop:disable all`) and `Lint/RedundantCopDisableDirective`.
- [ ] Pending-cops warning (`PendingCopsReporter`).
- [ ] Remaining `TargetRuby` sources (`Gemfile.lock`, `*.gemspec`,
      `RUBOCOP_TARGET_RUBY_VERSION`), and passing the version to Prism.
- [ ] `Lint/Syntax` messages: RuboCop uses the `parser` gem below
      `TargetRubyVersion` 3.4 and Prism from 3.4 on. We always use Prism.
- [ ] Remaining CLI options (`--auto-gen-config`, `--format` for every
      formatter, `--out`, `--fail-level`, `--display-only-*`, `--cache`, ...).
- [ ] Result cache, and parallel inspection (rayon).
- [ ] Hidden-file and `RubyInterpreters` edge cases in `TargetFinder`.
- [ ] A node-pattern-style matching layer over Prism nodes. The rubocop-ast
      node types (`send`, `csend`, `lvasgn`, ...) don't map one-to-one onto
      Prism's, so cops are ported against Prism's node types instead.

## oxcop: cops

Port department by department, using `bundle exec rake spec` as the
scoreboard. Start with the cops `../code` enables (its config uses
`DisabledByDefault: true` and opts into specific cops).

## Formatter: syntax_tree drop-in (second gem)

Goal: replace `../code`'s `bin/rails stree_new:{write,check}`
(`lib/tasks/stree_new.rake`) with a Rust binary:

- syntax_tree 6.3.0 printing (print width and plugins from `.streerc`) for
  `.rb`, `Gemfile`, `.rake`, ...
- `.erb` via w_syntax_tree-erb 0.12.0 semantics.
- `<script>`/`<style>` bodies and whole `.js/.css/.json.erb` files formatted by
  oxfmt (see `lib/tasks/support/erb_embedded_formatter.rb` in `../code`).
- `.streerc` `--ignore-files` handling, git-based file discovery, and
  `*:modified` variants that diff against the merge-base with `main`.
- `write` and `check` modes with the same messages and exit codes.

Clone syntax_tree (and w_syntax_tree-erb) as siblings when this starts.
Parsing, file discovery and shared infrastructure should move into a shared
crate at that point.
