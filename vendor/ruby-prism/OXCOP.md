# ruby-prism 1.9.0, vendored for oxcop

Based on the crates.io release `ruby-prism` 1.9.0 (MIT, from
github.com/ruby/prism), unmodified except for the additions below. It is a
vendored change only; it is not intended for upstream.

## Additions (the only new `unsafe` code)

In `src/lib.rs`:

- `ParseOptions` (a plain struct) and `parse_with_options(source, &options)`:
  like `parse`, but sets `pm_options_t.version` (the target Ruby version) and,
  when `lex` is set, registers a `pm_lex_callback_t` on the parser.
- `lex_token_callback` (`unsafe extern "C"`): the callback, which records each
  token's type, byte offsets and `parser->lex_state`.
- `LexToken` (a plain struct) and `ParseResult::tokens()`, a safe accessor for
  the recorded tokens. `ParseResult` gained two private fields that keep the
  options and the token buffer alive for the parser's lifetime.

In `build.rs`: the generated `Node` enum and node structs derive `Clone, Copy`
(as `Location` now does) — no `unsafe` involved.

## Why

The published crate cannot set the target Ruby version and does not expose the
lexer's tokens. RuboCop's `processed_source.tokens` (built by
`Prism::Translation::Parser::Lexer` from exactly this token stream) is used by
many layout cops and by `rubocop:disable` comment handling, and parsing must
follow `TargetRubyVersion` (e.g. `it` in Ruby 3.3 vs 3.4).

oxcop (`#![forbid(unsafe_code)]`) only uses the safe API, through
`crates/oxcop/src/prism.rs`.

This crate is its own Cargo workspace and is excluded from oxruby's, so clippy
and rustfmt (`rustfmt.toml`) leave the upstream code alone.
