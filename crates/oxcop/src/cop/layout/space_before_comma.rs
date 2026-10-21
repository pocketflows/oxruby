//! Port of `lib/rubocop/cop/layout/space_before_comma.rb`.

use crate::ast::token::Token;
use crate::cop::mixin::space_before_punctuation;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SpaceBeforeComma;

impl Cop for SpaceBeforeComma {
    fn name(&self) -> &'static str {
        "Layout/SpaceBeforeComma"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        space_before_punctuation::on_new_investigation(cx, kind);
    }
}

fn kind(token: &Token) -> Option<&'static str> {
    token.is_comma().then_some("comma")
}
