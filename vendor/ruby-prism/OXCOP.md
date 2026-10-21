# ruby-prism 1.9.0, vendored for oxcop

Copied from crates.io (`ruby-prism` 1.9.0, MIT) with these additions:

- `parse_with_options`: parses with a target Ruby version
  (`pm_options_t.version`) and can record the lexer's token stream
  (`ParseResult::tokens`, the data `Prism.lex` returns).
- `#[derive(Clone, Copy)]` on `Location`, `Node` and every node struct
  (`build.rs`).

It is its own Cargo workspace and is excluded from oxruby's, so clippy and
rustfmt (`rustfmt.toml`) leave the upstream code alone.
