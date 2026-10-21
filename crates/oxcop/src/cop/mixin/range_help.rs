//! Port of `lib/rubocop/cop/mixin/range_help.rb`.

use crate::ast::Node;
use crate::processed_source::{ProcessedSource, Range};

pub const BYTE_ORDER_MARK: char = '\u{feff}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Both,
    Left,
    Right,
}

/// Options of `range_with_surrounding_space`.
#[derive(Debug, Clone, Copy)]
pub struct SurroundingSpace {
    pub side: Side,
    pub newlines: bool,
    pub whitespace: bool,
    pub continuations: bool,
}

impl Default for SurroundingSpace {
    fn default() -> Self {
        Self {
            side: Side::Both,
            newlines: true,
            whitespace: false,
            continuations: false,
        }
    }
}

impl SurroundingSpace {
    pub fn side(side: Side) -> Self {
        Self {
            side,
            ..Self::default()
        }
    }
}

/// `source_range(buffer, line, column, length)`: `line` is 1-based, or 0
/// for the start of the buffer; `column` in characters.
pub fn source_range(ps: &ProcessedSource<'_>, line: usize, column: usize, length: usize) -> Range {
    let line_begin = if line == 0 { 0 } else { ps.line_start(line) };
    let begin = char_offset(ps.buffer_source(), line_begin, column);
    let end = char_offset(ps.buffer_source(), begin, length);
    Range::new(begin, end)
}

/// The byte offset `chars` characters after `from`.
pub fn char_offset(src: &str, from: usize, chars: usize) -> usize {
    src[from..]
        .char_indices()
        .nth(chars)
        .map_or(src.len().min(from + chars), |(i, _)| from + i)
}

/// `contents_range(node)`: between `loc.begin` and `loc.end`.
pub fn contents_range(node: Node<'_>) -> Range {
    Range::new(
        node.loc().begin.unwrap().end_pos,
        node.loc().end.unwrap().begin_pos,
    )
}

/// `arguments_range(node)`.
pub fn arguments_range(node: Node<'_>) -> Range {
    node.first_argument()
        .unwrap()
        .source_range()
        .join(node.last_argument().unwrap().source_range())
}

pub fn range_between(begin_pos: usize, end_pos: usize) -> Range {
    Range::new(begin_pos, end_pos)
}

fn directions(side: Side) -> (bool, bool) {
    match side {
        Side::Both => (true, true),
        Side::Left => (true, false),
        Side::Right => (false, true),
    }
}

fn move_pos(
    src: &[u8],
    mut pos: usize,
    step: isize,
    condition: bool,
    pred: impl Fn(u8) -> bool,
) -> usize {
    if !condition {
        return pos;
    }
    loop {
        let index = if step < 0 {
            pos.checked_sub(1)
        } else {
            Some(pos)
        };
        match index.and_then(|i| src.get(i)) {
            Some(&c) if pred(c) => {
                if step < 0 {
                    pos -= 1;
                } else {
                    pos += 1;
                }
            }
            _ => return pos,
        }
    }
}

fn move_pos_str(src: &[u8], mut pos: usize, step: isize, condition: bool, needle: &[u8]) -> usize {
    if !condition {
        return pos;
    }
    let size = needle.len();
    loop {
        let start = if step < 0 {
            pos.checked_sub(size)
        } else {
            Some(pos)
        };
        match start {
            Some(s) if src.get(s..s + size) == Some(needle) => {
                if step < 0 {
                    pos -= size;
                } else {
                    pos += size;
                }
            }
            _ => return pos,
        }
    }
}

fn final_pos(src: &[u8], pos: usize, step: isize, opts: SurroundingSpace) -> usize {
    let pos = move_pos(src, pos, step, true, |c| c == b' ' || c == b'\t');
    let pos = move_pos_str(src, pos, step, opts.continuations, b"\\\n");
    let pos = move_pos(src, pos, step, opts.newlines, |c| c == b'\n');
    move_pos(src, pos, step, opts.whitespace, |c| {
        matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
    })
}

/// `range_with_surrounding_space(range, side:, newlines:, whitespace:,
/// continuations:)`.
pub fn range_with_surrounding_space(src: &str, range: Range, opts: SurroundingSpace) -> Range {
    let src = src.as_bytes();
    let (go_left, go_right) = directions(opts.side);
    let begin = if go_left {
        final_pos(src, range.begin_pos, -1, opts)
    } else {
        range.begin_pos
    };
    let end = if go_right {
        final_pos(src, range.end_pos, 1, opts)
    } else {
        range.end_pos
    };
    Range::new(begin, end)
}

/// `range_with_surrounding_comma(range, side)`.
pub fn range_with_surrounding_comma(src: &str, range: Range, side: Side) -> Range {
    let src = src.as_bytes();
    let (go_left, go_right) = directions(side);
    let begin = move_pos(src, range.begin_pos, -1, go_left, |c| c == b',');
    let end = move_pos(src, range.end_pos, 1, go_right, |c| c == b',');
    Range::new(begin, end)
}

/// `range_by_whole_lines(range, include_final_newline:)`.
pub fn range_by_whole_lines(
    ps: &ProcessedSource<'_>,
    range: Range,
    include_final_newline: bool,
) -> Range {
    let first_line = ps.line_of(range.begin_pos);
    let last_line = ps.line_of(range.end_pos);
    let begin = ps.line_start(first_line);
    let mut end = ps.line_range(last_line).end_pos;
    if include_final_newline {
        end += 1;
    }
    let len = ps.buffer_source().len();
    Range::new(begin.min(len), end.min(len))
}

/// `column_offset_between(base_range, range)`.
pub fn column_offset_between(ps: &ProcessedSource<'_>, base: Range, range: Range) -> isize {
    effective_column(ps, base) as isize - effective_column(ps, range) as isize
}

/// `effective_column(range)`: the column, ignoring a byte order mark.
pub fn effective_column(ps: &ProcessedSource<'_>, range: Range) -> usize {
    let column = ps.column_of(range.begin_pos);
    if ps.line_of(range.begin_pos) == 1 && ps.raw_source().starts_with(BYTE_ORDER_MARK) {
        column.saturating_sub(1)
    } else {
        column
    }
}

/// `range_with_comments(node)`.
pub fn range_with_comments(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    let mut result = node.source_range();
    for comment in ps.node_comments(node) {
        result = result.join(comment.range);
    }
    result
}

/// `range_with_comments_and_lines(node)`.
pub fn range_with_comments_and_lines(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    range_by_whole_lines(ps, range_with_comments(ps, node), true)
}
