//! Port of `lib/rubocop/cop/style/frozen_string_literal_comment.rb`.

use crate::ast::lexer::Token;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::frozen_string_literal::{
    FROZEN_STRING_LITERAL_ENABLED, frozen_string_literal_comment_exists,
    frozen_string_literal_specified, frozen_string_literals_enabled,
};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_with_surrounding_space, source_range,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::magic_comment::MagicComment;
use crate::processed_source::ProcessedSource;
use crate::regex;

const MSG_MISSING_TRUE: &str = "Missing magic comment `# frozen_string_literal: true`.";
const MSG_MISSING: &str = "Missing frozen string literal comment.";
const MSG_UNNECESSARY: &str = "Unnecessary frozen string literal comment.";
const MSG_DISABLED: &str = "Frozen string literal comment must be set to `true`.";
const SHEBANG: &str = "#!";

#[derive(Default)]
pub struct FrozenStringLiteralComment;

impl Cop for FrozenStringLiteralComment {
    fn name(&self) -> &'static str {
        "Style/FrozenStringLiteralComment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.3)
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.tokens().is_empty() {
            return;
        }
        match style(cx) {
            "never" => {
                if frozen_string_literal_comment_exists(ps) {
                    unnecessary_comment_offense(cx);
                }
            }
            "always_true" => {
                if frozen_string_literal_specified(ps) {
                    if frozen_string_literals_enabled(cx) {
                        return;
                    }
                    // The comment exists, but is not enabled.
                    disabled_offense(cx);
                } else {
                    // The comment doesn't exist at all.
                    missing_offense(cx, MSG_MISSING_TRUE);
                }
            }
            _ => {
                if !frozen_string_literal_comment_exists(ps) {
                    missing_offense(cx, MSG_MISSING);
                }
            }
        }
    }
}

fn last_special_comment<'p>(ps: &'p ProcessedSource<'_>) -> Option<&'p Token> {
    let tokens = ps.tokens();
    let mut token_number = 0;
    let mut token = None;
    if tokens[token_number].text.starts_with(SHEBANG) {
        token = Some(&tokens[token_number]);
        token_number += 1;
    }
    if let Some(next_token) = tokens.get(token_number)
        && next_token.raw.is_none()
        && regex!(r"#.*coding\s?[:=]\s?(?:UTF|utf)-8")
            .find(&next_token.text)
            .is_some()
    {
        token = Some(next_token);
    }
    token
}

fn frozen_string_literal_comment<'p>(ps: &'p ProcessedSource<'_>) -> Option<&'p Token> {
    ps.tokens()
        .iter()
        .find(|token| MagicComment::parse(&token.text).frozen_string_literal_specified())
}

fn missing_offense(cx: &mut Investigation<'_, '_>, message: &str) {
    let ps = cx.processed_source;
    // `source_range(buffer, 0, 0)`: `length` defaults to 1.
    let range = source_range(ps, 0, 0, 1);
    cx.add_offense_with_correction(range, message, |corrector| {
        insert_comment(ps, corrector);
    });
}

fn unnecessary_comment_offense(cx: &mut Investigation<'_, '_>) {
    let ps = cx.processed_source;
    let Some(comment) = frozen_string_literal_comment(ps) else {
        return;
    };
    let range = comment.range;
    cx.add_offense_with_correction(range, MSG_UNNECESSARY, |corrector| {
        corrector.remove(range_with_surrounding_space(
            ps.buffer_source(),
            range,
            SurroundingSpace::side(Side::Right),
        ));
    });
}

fn disabled_offense(cx: &mut Investigation<'_, '_>) {
    let ps = cx.processed_source;
    let Some(comment) = frozen_string_literal_comment(ps) else {
        return;
    };
    cx.add_offense_with_correction(comment.range, MSG_DISABLED, |corrector| {
        enable_comment(ps, corrector, comment);
    });
}

/// `MagicComment#new_frozen_string_literal(true)`.
fn new_frozen_string_literal(text: &str) -> String {
    if regex!(r"-\*-(?<token>.+)-\*-").find(text).is_some() {
        "# -*- frozen_string_literal: true -*-".to_owned()
    } else {
        "# frozen_string_literal: true".to_owned()
    }
}

fn enable_comment(ps: &ProcessedSource<'_>, corrector: &mut Corrector, comment: &Token) {
    let line = ps.line_of(comment.range.begin_pos);
    corrector.replace(ps.line_range(line), new_frozen_string_literal(&comment.text));
}

fn insert_comment(ps: &ProcessedSource<'_>, corrector: &mut Corrector) {
    match last_special_comment(ps) {
        Some(comment) => {
            let line = ps.line_of(comment.range.begin_pos);
            corrector.insert_after(
                ps.line_range(line),
                format!("\n{FROZEN_STRING_LITERAL_ENABLED}"),
            );
        }
        None => {
            let len = ps.buffer_source().len();
            corrector.insert_before(
                crate::processed_source::Range::new(0, len),
                format!("{FROZEN_STRING_LITERAL_ENABLED}\n"),
            );
        }
    }
}
