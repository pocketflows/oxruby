//! Port of `lib/rubocop/cop/mixin/space_after_punctuation.rb`.

use crate::ast::token::Token;
use crate::cop::Investigation;
use crate::cop::correctors::punctuation_corrector;
use crate::processed_source::ProcessedSource;

/// The methods a cop including the mixin provides or overrides.
pub trait SpaceAfterPunctuation {
    /// `kind(token, next_token)`.
    fn kind(&self, token: &Token, next_token: &Token) -> Option<&'static str>;

    /// `space_style_before_rcurly`.
    fn space_style_before_rcurly<'a>(&self, cx: &Investigation<'a, '_>) -> &'a str;

    /// `space_missing?(token1, token2)`.
    fn space_missing(&self, ps: &ProcessedSource<'_>, token1: &Token, token2: &Token) -> bool {
        default_space_missing(ps, token1, token2, self.offset())
    }

    /// `offset`.
    fn offset(&self) -> usize {
        1
    }
}

/// The mixin's own `space_missing?` (what `super` calls).
pub fn default_space_missing(
    ps: &ProcessedSource<'_>,
    token1: &Token,
    token2: &Token,
    offset: usize,
) -> bool {
    token1.line(ps) == token2.line(ps) && token2.column(ps) == token1.column(ps) + offset
}

/// `on_new_investigation`.
pub fn on_new_investigation<C: SpaceAfterPunctuation>(cop: &C, cx: &mut Investigation<'_, '_>) {
    let ps = cx.processed_source;
    for pair in ps.tokens().windows(2) {
        let (token1, token2) = (&pair[0], &pair[1]);
        let Some(kind) = cop.kind(token1, token2) else {
            continue;
        };
        if !cop.space_missing(ps, token1, token2) {
            continue;
        }
        if !space_required_before(cop, cx, token2) {
            continue;
        }
        let pos = token1.range;
        let source = ps.slice(pos);
        cx.add_offense_with_correction(pos, &format!("Space missing after {kind}."), |corrector| {
            punctuation_corrector::add_space(corrector, pos, source);
        });
    }
}

fn space_required_before<C: SpaceAfterPunctuation>(
    cop: &C,
    cx: &Investigation<'_, '_>,
    token: &Token,
) -> bool {
    !(allowed_type(token)
        || (token.is_right_curly_brace() && cop.space_style_before_rcurly(cx) == "no_space"))
}

fn allowed_type(token: &Token) -> bool {
    matches!(token.ty, "tRPAREN" | "tRBRACK" | "tPIPE" | "tSTRING_DEND")
}
