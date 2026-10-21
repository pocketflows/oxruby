# Agent guide for oxruby

oxruby is a Rust port of RuboCop (the `oxcop` gem) and of syntax_tree plus
its ERB plugin (the `oxtree` crate and gem). Read `README.md` for the
layout, `docs/ROADMAP.md` (oxcop) and `docs/OXTREE.md` (oxtree) for status
and known gaps. When several agents work at once, `docs/PARALLEL.md` is the
contract: worktrees, ownership, and landing on main with `script/land`.

## Commands

```sh
cargo build -p oxcop                     # target/debug/oxcop (used by specs)
cargo test                               # Rust unit tests
bundle exec rake                         # everything CI runs
bundle exec rspec spec/rubocop/cop/layout/trailing_whitespace_spec.rb
bundle exec rake compare                 # diff vs real rubocop on ../code
script/land                              # gate and land your branch on main
```

## Porting rules

- **Port, don't redesign.** Each Rust file corresponds to a RuboCop file:
  `../rubocop/lib/rubocop/cop/layout/trailing_whitespace.rb` becomes
  `crates/oxcop/src/cop/layout/trailing_whitespace.rs`. Keep the same method
  names and order where practical, and start each file with a `//! Port of`
  doc comment naming its source. Messages are copied verbatim.
- Positions are byte offsets internally. Anything reported (columns, lengths)
  is in characters, as in RuboCop.
- `build.rs` registers every `cop/<department>/<name>.rs` file, so a cop is
  live (and its upstream spec runs) as soon as the file exists. Only land a
  cop once its whole spec passes.
- **No `unsafe` in code we write**, including patches to vendored crates.
  Every crate root starts with `#![forbid(unsafe_code)]`. Use safe std APIs
  (`str::from_utf8`, `char_indices`, checked slicing). Crates.io dependencies
  are fine even though they use `unsafe` internally; call their safe APIs and
  never write FFI glue. Prism comes from crates.io `ruby-prism` (pinned).
- `../rubocop` is checked out at the tracked tag (see `version.rs`). Read the
  Ruby source and its spec before porting a cop.

## Specs

- `spec/rubocop/` and `spec/support/` are vendored from upstream: **never edit
  them**. Re-sync with `rake vendor:rubocop`. If an upstream spec can't pass
  for a principled reason, record it (in your `docs/agents/<name>.md` when
  working in parallel). Don't change the spec.
- `spec/oxcop/` is ours: the shim that sends `expect_offense` and friends to
  the binary (`--stdin`, `-c <dumped spec config>`, `--only <cop>`).
- A cop is "done" when its upstream spec file passes in full.
