//! Port of `lib/rubocop/cop/layout/space_after_semicolon.rb`.

use crate::ast::token::Token;
use crate::cop::mixin::space_after_punctuation::{
    self, SpaceAfterPunctuation, default_space_missing,
};
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

#[derive(Default)]
pub struct SpaceAfterSemicolon;

impl Cop for SpaceAfterSemicolon {
    fn name(&self) -> &'static str {
        "Layout/SpaceAfterSemicolon"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        space_after_punctuation::on_new_investigation(self, cx);
    }
}

impl SpaceAfterPunctuation for SpaceAfterSemicolon {
    fn space_style_before_rcurly<'a>(&self, cx: &Investigation<'a, '_>) -> &'a str {
        cx.config
            .for_cop("Layout/SpaceInsideBlockBraces")
            .get("EnforcedStyle")
            .and_then(serde_yaml_ng::Value::as_str)
            .unwrap_or("space")
    }

    fn kind(&self, token: &Token, _next_token: &Token) -> Option<&'static str> {
        token.is_semicolon().then_some("semicolon")
    }

    fn space_missing(&self, ps: &ProcessedSource<'_>, token1: &Token, token2: &Token) -> bool {
        default_space_missing(ps, token1, token2, self.offset())
            && !semicolon_sequence(token1, token2)
    }
}

fn semicolon_sequence(token: &Token, next_token: &Token) -> bool {
    token.is_semicolon() && next_token.is_semicolon()
}
