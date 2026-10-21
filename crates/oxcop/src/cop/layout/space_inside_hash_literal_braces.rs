//! Port of `lib/rubocop/cop/layout/space_inside_hash_literal_braces.rb`.
//! The style-detection calls only feed `--auto-gen-config`.

use crate::ast::token::Token;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct SpaceInsideHashLiteralBraces;

impl Cop for SpaceInsideHashLiteralBraces {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideHashLiteralBraces"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash, NodeType::HashPattern]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let tokens = cx.processed_source.tokens_within(node.source_range());
        let (Some(first), Some(last)) = (tokens.first(), tokens.last()) else {
            return;
        };
        if !(first.is_left_brace() && last.is_right_curly_brace()) {
            return;
        }
        if tokens.len() >= 2 {
            check(cx, &tokens[0], &tokens[1]);
        }
        if tokens.len() > 2 {
            check(cx, &tokens[tokens.len() - 2], &tokens[tokens.len() - 1]);
        }
        if enforce_no_space_style_for_empty_braces(cx) {
            check_whitespace_only_hash(cx, node);
        }
    }
}

fn check(cx: &mut Investigation<'_, '_>, token1: &Token, token2: &Token) {
    let ps = cx.processed_source;
    if token1.line(ps) < token2.line(ps) {
        return;
    }
    if token2.is_comment() {
        return;
    }
    let is_empty_braces = token1.is_left_brace() && token2.is_right_curly_brace();
    let expect_space = expect_space(cx, token1, token2);
    if offense(cx, token1, expect_space) {
        incorrect_style_detected(cx, token1, token2, expect_space, is_empty_braces);
    }
}

fn expect_space(cx: &Investigation<'_, '_>, token1: &Token, token2: &Token) -> bool {
    let is_same_braces = token1.ty == token2.ty;
    let is_empty_braces = token1.is_left_brace() && token2.is_right_curly_brace();
    if is_same_braces && style(cx) == "compact" {
        false
    } else if is_empty_braces {
        !enforce_no_space_style_for_empty_braces(cx)
    } else {
        style(cx) != "no_space"
    }
}

fn incorrect_style_detected(
    cx: &mut Investigation<'_, '_>,
    token1: &Token,
    token2: &Token,
    expect_space: bool,
    is_empty_braces: bool,
) {
    let brace = if token1.is_left_brace() {
        token1.range
    } else {
        token2.range
    };
    let range = if expect_space {
        brace
    } else {
        space_range(cx, brace)
    };
    let message = message(cx, brace, is_empty_braces, expect_space);
    let source = cx.source(range);
    cx.add_offense_with_correction(range, &message, |corrector| {
        autocorrect(corrector, range, source);
    });
}

fn autocorrect(corrector: &mut Corrector, range: Range, source: &str) {
    if source
        .bytes()
        .any(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
    {
        corrector.remove(range);
    } else if source == "{" {
        corrector.insert_after(range, " ");
    } else {
        corrector.insert_before(range, " ");
    }
}

fn offense(cx: &Investigation<'_, '_>, token1: &Token, expect_space: bool) -> bool {
    let has_space = token1.is_space_after(cx.processed_source);
    if expect_space { !has_space } else { has_space }
}

fn message(cx: &Investigation<'_, '_>, brace: Range, is_empty_braces: bool, expect_space: bool) -> String {
    let inside_what = if is_empty_braces {
        "empty hash literal braces"
    } else {
        cx.source(brace)
    };
    let problem = if expect_space { "missing" } else { "detected" };
    format!("Space inside {inside_what} {problem}.")
}

fn space_range(cx: &Investigation<'_, '_>, token_range: Range) -> Range {
    let src = cx.processed_source.buffer_source().as_bytes();
    if cx.source(token_range) == "{" {
        let mut end_pos = token_range.end_pos;
        while matches!(src.get(end_pos), Some(b' ' | b'\t')) {
            end_pos += 1;
        }
        range_between(token_range.begin_pos + 1, end_pos)
    } else {
        let mut begin_pos = token_range.begin_pos;
        while begin_pos > 0 && matches!(src.get(begin_pos - 1), Some(b' ' | b'\t')) {
            begin_pos -= 1;
        }
        range_between(begin_pos, token_range.end_pos - 1)
    }
}

fn check_whitespace_only_hash(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let range = range_inside_hash(node);
    let source = cx.source(range);
    if source.is_empty()
        || !source
            .bytes()
            .all(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
    {
        return;
    }
    cx.add_offense_with_correction(
        range,
        "Space inside empty hash literal braces detected.",
        |corrector| corrector.remove(range),
    );
}

fn range_inside_hash(node: Node<'_>) -> Range {
    match (node.loc().begin, node.loc().end) {
        (Some(begin), Some(end)) => range_between(begin.end_pos, end.begin_pos),
        _ => node.source_range(),
    }
}

fn enforce_no_space_style_for_empty_braces(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyleForEmptyBraces") == Some("no_space")
}
