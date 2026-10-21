//! Port of `lib/rubocop/cop/layout/space_before_semicolon.rb`.

use crate::ast::token::Token;
use crate::cop::mixin::space_before_punctuation;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SpaceBeforeSemicolon;

impl Cop for SpaceBeforeSemicolon {
    fn name(&self) -> &'static str {
        "Layout/SpaceBeforeSemicolon"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        space_before_punctuation::on_new_investigation(cx, kind);
    }
}

fn kind(token: &Token) -> Option<&'static str> {
    token.is_semicolon().then_some("semicolon")
}
