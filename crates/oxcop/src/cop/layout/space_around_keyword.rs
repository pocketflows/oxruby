//! Port of `lib/rubocop/cop/layout/space_around_keyword.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const DO: &str = "do";
const SAFE_NAVIGATION: &str = "&.";
const NAMESPACE_OPERATOR: &str = "::";
const ACCEPT_LEFT_PAREN: &[&str] = &["break", "defined?", "next", "not", "rescue", "super", "yield"];
const ACCEPT_LEFT_SQUARE_BRACKET: &[&str] = &["super", "yield"];
const ACCEPT_NAMESPACE_OPERATOR: &str = "super";

/// The `node.loc` fields `check` looks at.
#[derive(Clone, Copy)]
enum Field {
    Operator,
    Begin,
    End,
    Keyword,
    Else,
    Selector,
}

#[derive(Default)]
pub struct SpaceAroundKeyword;

impl Cop for SpaceAroundKeyword {
    fn name(&self) -> &'static str {
        "Layout/SpaceAroundKeyword"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            And, Block, Numblock, Itblock, Break, Case, CaseMatch, Ensure, For, If, IfGuard,
            InPattern, Kwbegin, MatchPattern, MatchPatternP, Next, Or, Postexe, Preexe, Resbody,
            Rescue, Return, Send, Super, Zsuper, UnlessGuard, Until, When, While, Yield, Defined,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use Field::*;
        use NodeType as T;
        match node.ty() {
            T::And | T::Or => {
                if node.is_keyword() {
                    check(cx, node, &[Operator], Some(DO));
                }
            }
            T::Block | T::Numblock | T::Itblock | T::For => check(cx, node, &[Begin, End], Some(DO)),
            T::Case | T::CaseMatch => check(cx, node, &[Keyword, Else], Some(DO)),
            T::If => check(cx, node, &[Keyword, Else, Begin, End], Some("then")),
            T::Kwbegin => check(cx, node, &[Begin, End], None),
            T::MatchPattern => {
                if cx.target_ruby_version() >= 3.0 {
                    return;
                }
                check(cx, node, &[Operator], Some(DO));
            }
            T::MatchPatternP => check(cx, node, &[Operator], Some(DO)),
            T::Rescue => check(cx, node, &[Else], Some(DO)),
            T::Send => {
                if node.is_prefix_not() {
                    check(cx, node, &[Selector], Some(DO));
                }
            }
            T::Until | T::While => check(cx, node, &[Begin, End, Keyword], Some(DO)),
            _ => check(cx, node, &[Keyword], Some(DO)),
        }
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>, locations: &[Field], begin_keyword: Option<&str>) {
    for &field in locations {
        let loc = node.loc();
        let range = match field {
            Field::Operator => loc.operator,
            Field::Begin => loc.begin,
            Field::End => loc.end,
            Field::Keyword => loc.keyword,
            Field::Else => loc.else_,
            Field::Selector => loc.selector,
        };
        let Some(range) = range else {
            continue;
        };
        match field {
            Field::Begin => check_begin(cx, node, range, begin_keyword),
            Field::End => check_end(cx, node, range, begin_keyword),
            _ => check_keyword(cx, node, range),
        }
    }
}

fn check_begin(cx: &mut Investigation<'_, '_>, node: Node<'_>, range: Range, begin_keyword: Option<&str>) {
    if let Some(begin_keyword) = begin_keyword
        && cx.source(range) != begin_keyword
    {
        return;
    }
    check_keyword(cx, node, range);
}

fn check_end(cx: &mut Investigation<'_, '_>, node: Node<'_>, range: Range, begin_keyword: Option<&str>) {
    if begin_keyword == Some(DO) && !is_do(node) {
        return;
    }
    if !space_before_missing(cx, range) {
        return;
    }
    let message = format!("Space before keyword `{}` is missing.", cx.source(range));
    cx.add_offense_with_correction(range, &message, |corrector| {
        corrector.insert_before(range, " ");
    });
}

fn is_do(node: Node<'_>) -> bool {
    node.loc_is(node.loc().begin, DO)
}

fn check_keyword(cx: &mut Investigation<'_, '_>, node: Node<'_>, range: Range) {
    if space_before_missing(cx, range) && !preceded_by_operator(node) {
        let message = format!("Space before keyword `{}` is missing.", cx.source(range));
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.insert_before(range, " ");
        });
    }
    if !space_after_missing(cx, range) {
        return;
    }
    let message = format!("Space after keyword `{}` is missing.", cx.source(range));
    cx.add_offense_with_correction(range, &message, |corrector| {
        corrector.insert_after(range, " ");
    });
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
}

fn space_before_missing(cx: &Investigation<'_, '_>, range: Range) -> bool {
    let Some(pos) = range.begin_pos.checked_sub(1) else {
        return false;
    };
    let c = cx.processed_source.buffer_source().as_bytes()[pos];
    !(is_space(c) || b"(|{[;,*=".contains(&c))
}

fn space_after_missing(cx: &Investigation<'_, '_>, range: Range) -> bool {
    let src = cx.processed_source.buffer_source();
    let pos = range.end_pos;
    let Some(&c) = src.as_bytes().get(pos) else {
        // `accepted_opening_delimiter?` is true without a next character.
        return false;
    };
    let source = cx.source(range);
    if accepted_opening_delimiter(source, c) {
        return false;
    }
    let rest = &src.as_bytes()[pos..];
    if rest.starts_with(SAFE_NAVIGATION.as_bytes()) {
        return false;
    }
    if source == ACCEPT_NAMESPACE_OPERATOR && rest.starts_with(NAMESPACE_OPERATOR.as_bytes()) {
        return false;
    }
    !(is_space(c) || b";,#\\)}].".contains(&c))
}

fn accepted_opening_delimiter(source: &str, c: u8) -> bool {
    (ACCEPT_LEFT_SQUARE_BRACKET.contains(&source) && c == b'[')
        || (ACCEPT_LEFT_PAREN.contains(&source) && c == b'(')
}

fn preceded_by_operator(node: Node<'_>) -> bool {
    for ancestor in node.ancestors() {
        if ancestor.is_operator_keyword() || ancestor.is_range() {
            return true;
        }
        if !ancestor.is_send() {
            return false;
        }
        if ancestor.is_operator_method() {
            return true;
        }
    }
    false
}
