//! Port of `lib/rubocop/cop/mixin/space_before_punctuation.rb`.

use crate::ast::token::Token;
use crate::cop::Investigation;
use crate::cop::correctors::punctuation_corrector;
use crate::cop::mixin::range_help::range_between;

/// `on_new_investigation`; `kind` is the cop's `kind(token)`.
pub fn on_new_investigation(
    cx: &mut Investigation<'_, '_>,
    kind: fn(&Token) -> Option<&'static str>,
) {
    let ps = cx.processed_source;
    for pair in ps.sorted_tokens().windows(2) {
        let (token1, token2) = (&pair[0], &pair[1]);
        let Some(kind) = kind(token2) else {
            continue;
        };
        if !space_missing(cx, token1, token2) {
            continue;
        }
        if space_required_after(cx, token1) {
            continue;
        }
        let pos_before = range_between(token1.end_pos(), token2.begin_pos());
        cx.add_offense_with_correction(
            pos_before,
            &format!("Space found before {kind}."),
            |corrector| {
                punctuation_corrector::remove_space(corrector, pos_before);
            },
        );
    }
}

fn space_missing(cx: &Investigation<'_, '_>, token1: &Token, token2: &Token) -> bool {
    let ps = cx.processed_source;
    token1.line(ps) == token2.line(ps) && token2.begin_pos() > token1.end_pos()
}

fn space_required_after(cx: &Investigation<'_, '_>, token: &Token) -> bool {
    token.is_left_curly_brace() && space_required_after_lcurly(cx)
}

fn space_required_after_lcurly(cx: &Investigation<'_, '_>) -> bool {
    let style = cx
        .config
        .for_cop("Layout/SpaceInsideBlockBraces")
        .get("EnforcedStyle")
        .and_then(serde_yaml_ng::Value::as_str)
        .unwrap_or("space");
    style == "space"
}
