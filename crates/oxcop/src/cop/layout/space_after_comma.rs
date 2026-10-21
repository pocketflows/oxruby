//! Port of `lib/rubocop/cop/layout/space_after_comma.rb`.

use crate::ast::token::Token;
use crate::cop::mixin::space_after_punctuation::{self, SpaceAfterPunctuation};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SpaceAfterComma;

impl Cop for SpaceAfterComma {
    fn name(&self) -> &'static str {
        "Layout/SpaceAfterComma"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        space_after_punctuation::on_new_investigation(self, cx);
    }
}

impl SpaceAfterPunctuation for SpaceAfterComma {
    fn space_style_before_rcurly<'a>(&self, cx: &Investigation<'a, '_>) -> &'a str {
        cx.config
            .for_cop("Layout/SpaceInsideHashLiteralBraces")
            .get("EnforcedStyle")
            .and_then(serde_yaml_ng::Value::as_str)
            .unwrap_or("space")
    }

    fn kind(&self, token: &Token, next_token: &Token) -> Option<&'static str> {
        (token.is_comma() && !next_token.is_semicolon()).then_some("comma")
    }
}
