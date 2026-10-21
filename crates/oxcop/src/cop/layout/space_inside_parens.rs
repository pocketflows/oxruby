//! Port of `lib/rubocop/cop/layout/space_inside_parens.rb`.

use crate::ast::token::Token;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Space inside parentheses detected.";
const MSG_SPACE: &str = "No space inside parentheses detected.";

#[derive(Default)]
pub struct SpaceInsideParens;

impl Cop for SpaceInsideParens {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideParens"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let tokens = cx.processed_source.sorted_tokens();
        match style(cx) {
            "space" => process_with_space_style(cx, tokens),
            "compact" => process_with_compact_style(cx, tokens),
            _ => correct_extraneous_space(cx, tokens),
        }
    }
}

fn process_with_space_style(cx: &mut Investigation<'_, '_>, tokens: &[Token]) {
    for pair in tokens.windows(2) {
        correct_extraneous_space_in_empty_parens(cx, &pair[0], &pair[1]);
        correct_missing_space(cx, &pair[0], &pair[1]);
    }
}

fn process_with_compact_style(cx: &mut Investigation<'_, '_>, tokens: &[Token]) {
    for pair in tokens.windows(2) {
        let (token1, token2) = (&pair[0], &pair[1]);
        correct_extraneous_space_in_empty_parens(cx, token1, token2);
        if !left_parens(token1, token2) && !right_parens(token1, token2) {
            correct_missing_space(cx, token1, token2);
        } else {
            correct_extraneous_space_between_consecutive_parens(cx, token1, token2);
        }
    }
}

fn remove_offense(cx: &mut Investigation<'_, '_>, range: Range) {
    cx.add_offense_with_correction(range, MSG, |corrector| corrector.remove(range));
}

fn correct_extraneous_space(cx: &mut Investigation<'_, '_>, tokens: &[Token]) {
    let ps = cx.processed_source;
    for pair in tokens.windows(2) {
        let (token1, token2) = (&pair[0], &pair[1]);
        if !parens(token1, token2) {
            continue;
        }
        if token2.is_comment() {
            continue;
        }
        if !(token1.line(ps) == token2.line(ps) && token1.is_space_after(ps)) {
            continue;
        }
        remove_offense(cx, range_between(token1.end_pos(), token2.begin_pos()));
    }
}

fn correct_extraneous_space_between_consecutive_parens(
    cx: &mut Investigation<'_, '_>,
    token1: &Token,
    token2: &Token,
) {
    let range = range_between(token1.end_pos(), token2.begin_pos());
    if cx.source(range) != " " {
        return;
    }
    remove_offense(cx, range);
}

fn correct_extraneous_space_in_empty_parens(
    cx: &mut Investigation<'_, '_>,
    token1: &Token,
    token2: &Token,
) {
    if !(token1.is_left_parens() && token2.is_right_parens()) {
        return;
    }
    if cx.source(range_between(token1.begin_pos(), token2.end_pos())) == "()" {
        return;
    }
    remove_offense(cx, range_between(token1.end_pos(), token2.begin_pos()));
}

fn correct_missing_space(cx: &mut Investigation<'_, '_>, token1: &Token, token2: &Token) {
    if can_be_ignored(cx, token1, token2) {
        return;
    }
    let range = if token1.is_left_parens() {
        range_between(token2.begin_pos(), token2.begin_pos() + 1)
    } else {
        range_between(token2.begin_pos(), token2.end_pos())
    };
    cx.add_offense_with_correction(range, MSG_SPACE, |corrector| {
        corrector.insert_before(range, " ");
    });
}

fn parens(token1: &Token, token2: &Token) -> bool {
    token1.is_left_parens() || token2.is_right_parens()
}

fn left_parens(token1: &Token, token2: &Token) -> bool {
    token1.is_left_parens() && token2.is_left_parens()
}

fn right_parens(token1: &Token, token2: &Token) -> bool {
    token1.is_right_parens() && token2.is_right_parens()
}

fn can_be_ignored(cx: &Investigation<'_, '_>, token1: &Token, token2: &Token) -> bool {
    let ps = cx.processed_source;
    if !parens(token1, token2) {
        return true;
    }
    if cx.source(range_between(token1.begin_pos(), token2.end_pos())) == "()" {
        return true;
    }
    if token2.is_comment() {
        return true;
    }
    token1.line(ps) != token2.line(ps) || token1.is_space_after(ps)
}
