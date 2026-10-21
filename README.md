# oxruby

Rust rewrites of Ruby's lint and format tooling, built on the
[Prism](https://github.com/ruby/prism) parser and shipped as ordinary gems
with prebuilt native binaries.

| Gem | Replaces | Status |
| --- | --- | --- |
| [`oxcop`](gems/oxcop) | [RuboCop](https://github.com/rubocop/rubocop) 1.91.0 | All 613 core cops, each passing its whole upstream spec; matches RuboCop on a 5,692-file Rails app. See [docs/ROADMAP.md](docs/ROADMAP.md) |
| [`oxtree`](gems/oxtree) | [syntax_tree](https://github.com/ruby-syntax-tree/syntax_tree) 6.3.0 + ERB | Formatter, `oxtree` CLI and `stree-new` mode; byte-identical to syntax_tree on every corpus tried. See [docs/OXTREE.md](docs/OXTREE.md) |

## oxcop

```ruby
# Gemfile
gem "oxcop", require: false
```

```sh
bundle exec oxcop [options] [paths]   # same options, .rubocop.yml and output as rubocop
```

oxcop aims to be a drop-in replacement: the same `.rubocop.yml`, the same
command-line options, byte-identical output from the built-in formatters, and
the same exit codes (0 clean, 1 offenses, 2 error).

**Scope.** oxcop ports RuboCop's core cops. Ruby extensions cannot run inside a
Rust binary, so plugin cops (rubocop-rails, rubocop-rspec, custom cops) don't
run. Plugins' default configs *are* loaded through the gem's launcher, so
their excludes and core-cop settings apply. The intended use is running oxcop
for the core cops and RuboCop with `--only` for the rest, or plugin ports
later.

Ported cops: all 613 of RuboCop 1.91.0's core cops. Each passes its whole
upstream spec, 29,116 examples in all, including the project-index
(`UseProjectIndex`) examples, which oxcop runs on Shopify's rubydex crate.
`Lint/Syntax`'s spec drives RuboCop's Commissioner directly, so it never
reaches oxcop, and its messages come from Prism. `oxcop --show-ported-cops`
prints the list. See `docs/ROADMAP.md` for status, gaps and next steps.

## Layout

```
crates/oxcop/src/    Rust port; files mirror rubocop's lib/rubocop/ one-to-one
crates/oxtree/       syntax_tree + ERB port (see docs/OXTREE.md)
gems/oxcop/          the gem: Ruby launcher + per-platform binary under exe/<platform>/
gems/oxtree/         the oxtree gem, same layout
vendor/rubocop/      RuboCop's config/default.yml (MIT), embedded in the binary
spec/rubocop/cop/    RuboCop's cop specs, verbatim
spec/support/        RuboCop's shared examples for those specs, verbatim
spec/oxcop/          the shim that runs those specs against the oxcop binary
script/              differential testing against real RuboCop
```

The repository expects two sibling checkouts next to it:

```
../rubocop   upstream RuboCop at the tracked tag (reference for porting)
../code      a real Rails app used as a differential-testing corpus
```

## Testing

```sh
bundle install
bundle exec rake          # version check, cargo fmt/clippy/test, specs
bundle exec rake spec     # RuboCop's own cop specs, run against oxcop
bundle exec rake compare  # oxcop vs. real rubocop on ../code (DIR=... to override)
```

There are two acceptance bars:

1. **Upstream specs.** The ~30,000 examples in `spec/rubocop/cop/` are RuboCop's
   own, unmodified. `spec/oxcop/cop_helper.rb` replaces the investigation step
   of `expect_offense`/`expect_correction` with a call to the oxcop binary, so
   the examples assert on oxcop's offenses and corrections. Examples for cops
   that aren't ported yet show up as *pending*, so the pending count is the
   remaining work. The goal is 100% passing.
2. **Real-world parity.** `script/compare_with_rubocop` runs both tools on a
   real project (restricted to the ported cops) and diffs the JSON reports.

## Releasing

`rake gem:native[arm64-darwin]` builds a platform gem that contains the binary;
`rake gem:ruby` builds the fallback gem for other platforms, which tells the
user that no prebuilt binary exists. `.github/workflows/release.yml` builds
every platform and pushes on a `v*` tag.

## License

MIT. Portions are derived from RuboCop (MIT); see `LICENSE` and
`vendor/rubocop/LICENSE.txt`.
