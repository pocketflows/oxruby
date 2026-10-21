# Agent guide for oxruby

oxruby is a Rust port of RuboCop (the `oxcop` gem), with a syntax_tree
formatter port planned. Read `README.md` for the layout and
`docs/ROADMAP.md` for known gaps.

## Commands

```sh
cargo build                              # target/debug/oxcop (used by specs)
cargo test                               # Rust unit tests
bundle exec rake                         # everything CI runs
bundle exec rspec spec/rubocop/cop/layout/trailing_whitespace_spec.rb
bundle exec rake compare                 # diff vs real rubocop on ../code
```

## Porting rules

- **Port, don't redesign.** Each Rust file corresponds to a RuboCop file:
  `../rubocop/lib/rubocop/cop/layout/trailing_whitespace.rb` becomes
  `crates/oxcop/src/cop/layout/trailing_whitespace.rs`. Keep the same method
  names and order where practical, and start each file with a `//! Port of`
  doc comment naming its source. Messages are copied verbatim.
- Positions are byte offsets internally. Anything reported (columns, lengths)
  is in characters, as in RuboCop.
- Register new cops in `cop/registry.rs`; that also makes their upstream
  specs run instead of being skipped.
- **No `unsafe`.** The `unsafe` keyword never appears in our code. Every
  crate root starts with `#![forbid(unsafe_code)]`. Use safe std APIs
  instead (`str::from_utf8`, `char_indices`, checked slicing), and depend on
  other crates through their safe Rust APIs rather than writing FFI glue.
- `../rubocop` is checked out at the tracked tag (see `version.rs`). Read the
  Ruby source and its spec before porting a cop.

## Specs

- `spec/rubocop/` and `spec/support/` are vendored from upstream: **never edit
  them**. Re-sync with `rake vendor:rubocop`. If an upstream spec can't pass
  for a principled reason, record it in `docs/ROADMAP.md`. Don't change the
  spec.
- `spec/oxcop/` is ours: the shim that sends `expect_offense` and friends to
  the binary (`--stdin`, `-c <dumped spec config>`, `--only <cop>`).
- A cop is "done" when its upstream spec file passes in full.
