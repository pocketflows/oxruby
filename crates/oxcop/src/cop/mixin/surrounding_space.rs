//! Port of `lib/rubocop/cop/mixin/surrounding_space.rb`. The cop's
//! `autocorrect(corrector, node)` and its `@ignored_nodes` are the
//! [`SurroundingSpace`] trait.

use crate::ast::Node;
use crate::ast::token::Token;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

pub const NO_SPACE_COMMAND: &str = "Do not use";
pub const SPACE_COMMAND: &str = "Use";

/// Which side of a token `side_space_range` looks at (`:left`, `:right`, or
/// anything else for the token itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    None,
}

pub trait SurroundingSpace {
    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, node: Node<'_>);

    fn ignored_nodes(&mut self) -> &mut IgnoredNodes;
}

fn single_space(b: Option<&u8>) -> bool {
    matches!(b, Some(b' ' | b'\t'))
}

/// `side_space_range(range:, side:, include_newlines:)`.
pub fn side_space_range(
    ps: &ProcessedSource<'_>,
    range: Range,
    side: Side,
    include_newlines: bool,
) -> Range {
    let src = ps.buffer_source().as_bytes();
    let mut begin_pos = range.begin_pos;
    let mut end_pos = range.end_pos;
    if side == Side::Left {
        end_pos = begin_pos;
        begin_pos = reposition(src, begin_pos, -1, include_newlines);
    }
    if side == Side::Right {
        begin_pos = end_pos;
        end_pos = reposition(src, end_pos, 1, include_newlines);
    }
    Range::new(begin_pos, end_pos)
}

/// `reposition(src, pos, step, include_newlines:)`.
fn reposition(src: &[u8], pos: usize, step: isize, include_newlines: bool) -> usize {
    let offset: isize = if step == -1 { -1 } else { 0 };
    let mut pos = pos as isize;
    loop {
        let index = pos + offset;
        // `src[-1]` is the last character in Ruby.
        let c = if index < 0 {
            src.last()
        } else {
            src.get(index as usize)
        };
        if single_space(c) || (include_newlines && c == Some(&b'\n')) {
            pos += step;
        } else {
            break;
        }
        if pos < -1 || pos > src.len() as isize {
            break;
        }
    }
    pos.max(0) as usize
}

/// `extra_space?(token, side)`.
pub fn extra_space(ps: &ProcessedSource<'_>, token: Option<&Token>, side: Side) -> bool {
    let Some(token) = token else {
        return false;
    };
    let src = ps.buffer_source().as_bytes();
    if side == Side::Left {
        single_space(src.get(token.end_pos()))
    } else {
        let position = token.begin_pos().saturating_sub(1);
        single_space(src.get(position))
    }
}

/// `no_space_offenses(node, left_token, right_token, message, start_ok:,
/// end_ok:)`; `message` has a `%<command>s` placeholder, given as `{}`.
#[allow(clippy::too_many_arguments)]
pub fn no_space_offenses<'a, C: SurroundingSpace>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    left_token: Option<&Token>,
    right_token: Option<&Token>,
    message: &str,
    start_ok: bool,
    end_ok: bool,
) {
    let ps = cx.processed_source;
    if extra_space(ps, left_token, Side::Left)
        && !start_ok
        && let Some(left) = left_token
    {
        space_offense(cop, cx, node, left, Side::Right, message, NO_SPACE_COMMAND);
    }
    if !extra_space(ps, right_token, Side::Right) || end_ok {
        return;
    }
    if let Some(right) = right_token {
        space_offense(cop, cx, node, right, Side::Left, message, NO_SPACE_COMMAND);
    }
}

/// `space_offenses(node, left_token, right_token, message, start_ok:,
/// end_ok:)`.
#[allow(clippy::too_many_arguments)]
pub fn space_offenses<'a, C: SurroundingSpace>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    left_token: Option<&Token>,
    right_token: Option<&Token>,
    message: &str,
    start_ok: bool,
    end_ok: bool,
) {
    let ps = cx.processed_source;
    if !(extra_space(ps, left_token, Side::Left) || start_ok)
        && let Some(left) = left_token
    {
        space_offense(cop, cx, node, left, Side::None, message, SPACE_COMMAND);
    }
    if extra_space(ps, right_token, Side::Right) || end_ok {
        return;
    }
    if let Some(right) = right_token {
        space_offense(cop, cx, node, right, Side::None, message, SPACE_COMMAND);
    }
}

pub fn space_offense<'a, C: SurroundingSpace>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    token: &Token,
    side: Side,
    message: &str,
    command: &str,
) {
    let range = side_space_range(cx.processed_source, token.range, side, false);
    let message = message.replace("{}", command);
    cx.add_offense_cx(range, &message, |corrector, cx| {
        if !cop.ignored_nodes().contains(node) {
            cop.autocorrect(corrector, cx, node);
        }
        cop.ignored_nodes().ignore(node);
    });
}

/// `empty_offenses(node, left, right, message)`; `empty_config` is the
/// cop's `empty_config`.
pub fn empty_offenses<'a, C: SurroundingSpace>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    left: &Token,
    right: &Token,
    message: &str,
    empty_config: Option<&str>,
) {
    let range = range_between(left.begin_pos(), right.end_pos());
    let ps = cx.processed_source;
    if offending_empty_space(ps, empty_config, left, right) {
        empty_offense(cop, cx, node, range, message, "Use one");
    }
    if !offending_empty_no_space(empty_config, left, right) {
        return;
    }
    empty_offense(cop, cx, node, range, message, "Do not use");
}

fn empty_offense<'a, C: SurroundingSpace>(
    cop: &mut C,
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    range: Range,
    message: &str,
    command: &str,
) {
    let message = message.replace("{}", command);
    cx.add_offense_cx(range, &message, |corrector, cx| {
        cop.autocorrect(corrector, cx, node);
    });
}

/// `empty_brackets?(left, right, tokens:)`.
pub fn empty_brackets(left: Option<&Token>, right: Option<&Token>, tokens: &[Token]) -> bool {
    let left_index = left.and_then(|l| tokens.iter().position(|t| t == l));
    let right_index = right.and_then(|r| tokens.iter().position(|t| t == r));
    match right_index {
        Some(right_index) => left_index.map(|l| l + 1) == Some(right_index),
        None => false,
    }
}

/// `offending_empty_space?(config, left, right)`.
pub fn offending_empty_space(
    ps: &ProcessedSource<'_>,
    config: Option<&str>,
    left: &Token,
    right: &Token,
) -> bool {
    config == Some("space") && !space_between(ps, left, right)
}

/// `offending_empty_no_space?(config, left, right)`.
pub fn offending_empty_no_space(config: Option<&str>, left: &Token, right: &Token) -> bool {
    config == Some("no_space") && !no_character_between(left, right)
}

fn space_between(ps: &ProcessedSource<'_>, left: &Token, right: &Token) -> bool {
    left.end_pos() + 1 == right.begin_pos()
        && ps.buffer_source().as_bytes().get(left.end_pos()) == Some(&b' ')
}

fn no_character_between(left: &Token, right: &Token) -> bool {
    left.end_pos() == right.begin_pos()
}
