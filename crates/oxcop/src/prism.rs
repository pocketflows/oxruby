//! The one place oxcop calls into Prism's parse-with-options/lexing API
//! (the vendored addition documented in `vendor/ruby-prism/OXCOP.md`), so a
//! different token source could replace it.

pub use ruby_prism::ParseResult;

/// A raw Prism token: its `pm_token_type_t`, byte offsets and lexer state.
#[derive(Debug, Clone, Copy)]
pub struct RawToken {
    pub type_: u32,
    pub start: usize,
    pub end: usize,
    pub state: u32,
}

/// Parses `source` as the given Ruby version (`TargetRubyVersion`),
/// recording the lexer's tokens.
pub fn parse(source: &str, ruby_version: f64) -> ParseResult<'_> {
    let version = (
        ruby_version.trunc() as u8,
        ((ruby_version * 10.0).round() as u8) % 10,
    );
    ruby_prism::parse_with_options(
        source.as_bytes(),
        &ruby_prism::ParseOptions {
            version: Some(version),
            lex: true,
            marker: std::marker::PhantomData,
        },
    )
}

/// The tokens recorded while parsing.
pub fn tokens(result: &ParseResult<'_>) -> Vec<RawToken> {
    result
        .tokens()
        .iter()
        .map(|t| RawToken {
            type_: t.type_,
            start: t.start,
            end: t.end,
            state: t.state,
        })
        .collect()
}
