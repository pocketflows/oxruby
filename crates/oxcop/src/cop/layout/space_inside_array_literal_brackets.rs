//! Port of `lib/rubocop/cop/layout/space_inside_array_literal_brackets.rb`.

use crate::ast::token::Token;
use crate::ast::{Node, NodeType};
use crate::cop::correctors::space_corrector;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::surrounding_space::{
    NO_SPACE_COMMAND, Side, SurroundingSpace, empty_brackets, empty_offenses, no_space_offenses,
    side_space_range, space_offense, space_offenses,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "{} space inside array brackets.";
const EMPTY_MSG: &str = "{} space inside empty array brackets.";

#[derive(Default)]
pub struct SpaceInsideArrayLiteralBrackets {
    ignored_nodes: IgnoredNodes,
}

impl Cop for SpaceInsideArrayLiteralBrackets {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideArrayLiteralBrackets"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array, NodeType::ArrayPattern]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Array) && !node.is_square_brackets() {
            return;
        }
        let node = find_node_with_brackets(node);
        let ps = cx.processed_source;
        let (tokens, left, right) = array_brackets(ps, node);
        let (Some(left), Some(right)) = (left, right) else {
            return;
        };
        if empty_brackets(Some(left), Some(right), tokens) {
            let empty_config = empty_config(cx);
            empty_offenses(self, cx, node, left, right, EMPTY_MSG, empty_config);
            return;
        }
        let start_ok = next_to_newline(ps, tokens, left);
        let end_ok = if node.is_single_line() {
            false
        } else {
            end_has_own_line(ps, right)
        };
        self.issue_offenses(cx, node, tokens, left, right, start_ok, end_ok);
    }
}

impl SurroundingSpace for SpaceInsideArrayLiteralBrackets {
    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, node: Node<'_>) {
        let ps = cx.processed_source;
        let (tokens, left, right) = array_brackets(ps, node);
        let (Some(left), Some(right)) = (left, right) else {
            return;
        };
        if empty_brackets(Some(left), Some(right), tokens) {
            space_corrector::empty_corrections(ps, corrector, empty_config(cx), left, right);
        } else {
            match style(cx) {
                "no_space" => space_corrector::remove_space(ps, corrector, left, right),
                "space" => space_corrector::add_space(ps, corrector, left, right),
                _ => compact_corrections(ps, corrector, tokens, left, right),
            }
        }
    }

    fn ignored_nodes(&mut self) -> &mut IgnoredNodes {
        &mut self.ignored_nodes
    }
}

impl SpaceInsideArrayLiteralBrackets {
    #[allow(clippy::too_many_arguments)]
    fn issue_offenses<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        tokens: &[Token],
        left: &Token,
        right: &Token,
        start_ok: bool,
        end_ok: bool,
    ) {
        match style(cx) {
            "no_space" => {
                let start_ok = next_to_comment(tokens, left);
                no_space_offenses(self, cx, node, Some(left), Some(right), MSG, start_ok, end_ok);
            }
            "space" => {
                space_offenses(self, cx, node, Some(left), Some(right), MSG, start_ok, end_ok);
            }
            _ => self.compact_offenses(cx, node, tokens, left, right, start_ok, end_ok),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn compact_offenses<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        tokens: &[Token],
        left: &Token,
        right: &Token,
        start_ok: bool,
        end_ok: bool,
    ) {
        let ps = cx.processed_source;
        if qualifies_for_compact(ps, tokens, left, Side::Left) {
            space_offense(self, cx, node, left, Side::Right, MSG, NO_SPACE_COMMAND);
        } else if !multi_dimensional_array(tokens, left, Side::Left) {
            space_offenses(self, cx, node, Some(left), None, MSG, start_ok, true);
        }
        if qualifies_for_compact(ps, tokens, right, Side::Right) {
            space_offense(self, cx, node, right, Side::Left, MSG, NO_SPACE_COMMAND);
        } else if !multi_dimensional_array(tokens, right, Side::Right) {
            space_offenses(self, cx, node, None, Some(right), MSG, true, end_ok);
        }
    }
}

fn find_node_with_brackets(node: Node<'_>) -> Node<'_> {
    node.ancestors()
        .find(|a| a.is(NodeType::ConstPattern))
        .unwrap_or(node)
}

fn array_brackets<'p>(
    ps: &'p ProcessedSource<'_>,
    node: Node<'_>,
) -> (&'p [Token], Option<&'p Token>, Option<&'p Token>) {
    let tokens = ps.tokens_within(node.source_range());
    let left = tokens.iter().find(|t| t.is_left_bracket());
    let right = tokens.iter().rev().find(|t| t.is_right_bracket());
    (tokens, left, right)
}

fn empty_config<'a>(cx: &Investigation<'a, '_>) -> Option<&'a str> {
    cx.cop_config_str("EnforcedStyleForEmptyBrackets")
}

fn index_for(tokens: &[Token], token: &Token) -> usize {
    tokens.iter().position(|t| t == token).unwrap()
}

/// `tokens[i]` with Ruby's negative indexing.
fn token_at(tokens: &[Token], i: isize) -> Option<&Token> {
    if i < 0 {
        tokens.len().checked_sub(i.unsigned_abs()).map(|j| &tokens[j])
    } else {
        tokens.get(i as usize)
    }
}

fn next_to_newline(ps: &ProcessedSource<'_>, tokens: &[Token], token: &Token) -> bool {
    tokens
        .get(index_for(tokens, token) + 1)
        .is_some_and(|next| next.line(ps) != token.line(ps))
}

fn end_has_own_line(ps: &ProcessedSource<'_>, token: &Token) -> bool {
    let line = token.line(ps) - 1;
    let column = token.column(ps);
    if column == 0 {
        return true;
    }
    let col = column - 1;
    let text: String = ps.lines()[line].chars().take(col + 1).collect();
    !text
        .chars()
        .any(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
}

fn next_to_comment(tokens: &[Token], token: &Token) -> bool {
    tokens
        .get(index_for(tokens, token) + 1)
        .is_some_and(|t| t.is_comment())
}

fn qualifies_for_compact(ps: &ProcessedSource<'_>, tokens: &[Token], token: &Token, side: Side) -> bool {
    if side == Side::Right {
        multi_dimensional_array(tokens, token, side) && token.is_space_before(ps)
    } else {
        multi_dimensional_array(tokens, token, side) && token.is_space_after(ps)
    }
}

fn multi_dimensional_array(tokens: &[Token], token: &Token, side: Side) -> bool {
    let offset: isize = if side == Side::Right { -1 } else { 1 };
    let mut i = index_for(tokens, token) as isize + offset;
    while token_at(tokens, i).is_some_and(|t| t.is_new_line()) {
        i += offset;
    }
    match token_at(tokens, i) {
        Some(t) if side == Side::Right => t.is_right_bracket(),
        Some(t) => t.is_left_bracket(),
        None => false,
    }
}

fn compact_corrections(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    tokens: &[Token],
    left: &Token,
    right: &Token,
) {
    if multi_dimensional_array(tokens, left, Side::Left) {
        corrector.remove(side_space_range(ps, left.range, Side::Right, true));
    } else if !left.is_space_after(ps) {
        corrector.insert_after(left.range, " ");
    }
    if multi_dimensional_array(tokens, right, Side::Right) {
        corrector.remove(side_space_range(ps, right.range, Side::Left, true));
    } else if !right.is_space_before(ps) {
        corrector.insert_before(right.range, " ");
    }
}
