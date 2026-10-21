//! Port of `lib/rubocop/cop/mixin/heredoc.rb`.
//!
//! Cops call [`heredoc_node`] from `on_node` for the node types in
//! [`HEREDOC_NODE_TYPES`] (Ruby's `on_str`/`on_dstr`/`on_xstr`, which call
//! `on_heredoc` for heredocs).

use crate::ast::{Node, NodeType};
use crate::regex;

/// The node types `on_heredoc` is dispatched from.
pub const HEREDOC_NODE_TYPES: &[NodeType] = &[NodeType::Str, NodeType::Dstr, NodeType::Xstr];

/// `on_str`: the node when it is a heredoc.
pub fn heredoc_node(node: Node<'_>) -> Option<Node<'_>> {
    node.is_heredoc().then_some(node)
}

fn opening_delimiter() -> &'static onig::Regex {
    regex!(r#"(<<[~-]?)['"`]?([^'"`]+)['"`]?"#)
}

/// `indent_level(str)`: the smallest indentation of the lines of `str`
/// (lines consisting only of whitespace are ignored).
pub fn indent_level(str: &str) -> usize {
    str.split_inclusive('\n')
        .map(|line| {
            // `line[/^\s*/]`
            let end = line
                .bytes()
                .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
                .unwrap_or(line.len());
            &line[..end]
        })
        .filter(|indentation| !indentation.ends_with('\n'))
        .map(str::len)
        .min()
        .unwrap_or(0)
}

/// `delimiter_string(node)`, given the node's (or a range's) source.
pub fn delimiter_string(source: &str) -> &str {
    opening_delimiter()
        .captures(source)
        .and_then(|c| c.at(2))
        .unwrap_or("")
}

/// `heredoc_type(node)`, given the node's source: `<<`, `<<-` or `<<~`.
pub fn heredoc_type(source: &str) -> &str {
    opening_delimiter()
        .captures(source)
        .and_then(|c| c.at(1))
        .unwrap_or("")
}
