//! Port of `lib/rubocop/cop/mixin/frozen_string_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::magic_comment::MagicComment;
use crate::processed_source::ProcessedSource;

pub const FROZEN_STRING_LITERAL_ENABLED: &str = "# frozen_string_literal: true";

/// `leading_comment_lines`: the lines before the first non-comment token.
pub fn leading_comment_lines<'p>(ps: &'p ProcessedSource<'_>) -> &'p [&'p str] {
    let lines = ps.lines();
    match ps.tokens().iter().find(|t| t.ty != "tCOMMENT") {
        Some(token) => {
            let line = ps.line_of(token.range.begin_pos);
            &lines[..(line - 1).min(lines.len())]
        }
        None => lines,
    }
}

/// `leading_magic_comments`
pub fn leading_magic_comments<'p>(ps: &'p ProcessedSource<'_>) -> Vec<MagicComment<'p>> {
    leading_comment_lines(ps)
        .iter()
        .map(|line| MagicComment::parse(line))
        .collect()
}

/// `frozen_string_literal_comment_exists?`
pub fn frozen_string_literal_comment_exists(ps: &ProcessedSource<'_>) -> bool {
    leading_comment_lines(ps)
        .iter()
        .any(|line| MagicComment::parse(line).valid_literal_value())
}

/// `frozen_string_literals_enabled?`
pub fn frozen_string_literals_enabled(cx: &Investigation<'_, '_>) -> bool {
    let magic = leading_comment_lines(cx.processed_source)
        .iter()
        .map(|line| MagicComment::parse(line))
        .find(|m| m.frozen_string_literal_specified());
    if let Some(magic) = magic {
        return magic.is_frozen_string_literal();
    }
    string_literals_frozen_by_default(cx).unwrap_or(false)
}

/// `Config#string_literals_frozen_by_default?` (`AllCops:
/// StringLiteralsFrozenByDefault`).
pub fn string_literals_frozen_by_default(cx: &Investigation<'_, '_>) -> Option<bool> {
    cx.config
        .for_all_cops()
        .get("StringLiteralsFrozenByDefault")
        .and_then(serde_yaml_ng::Value::as_bool)
}

/// `frozen_string_literals_disabled?`
pub fn frozen_string_literals_disabled(ps: &ProcessedSource<'_>) -> bool {
    leading_comment_lines(ps).iter().any(|line| {
        MagicComment::parse(line).frozen_string_literal()
            == Some(crate::magic_comment::FrozenValue::False)
    })
}

/// `frozen_string_literal_specified?`
pub fn frozen_string_literal_specified(ps: &ProcessedSource<'_>) -> bool {
    leading_comment_lines(ps)
        .iter()
        .any(|line| MagicComment::parse(line).frozen_string_literal_specified())
}

pub fn uninterpolated_string(node: Node<'_>) -> bool {
    node.is(NodeType::Str)
        || (node.is(NodeType::Dstr)
            && !node.descendants().any(|d| {
                matches!(
                    d.ty(),
                    NodeType::Begin | NodeType::Ivar | NodeType::Cvar | NodeType::Gvar
                )
            }))
}

pub fn uninterpolated_heredoc(node: Node<'_>) -> bool {
    node.is(NodeType::Dstr) && node.is_heredoc() && node.child_nodes().all(|c| c.is(NodeType::Str))
}

/// `frozen_string_literal?(node)`.
pub fn frozen_string_literal(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let frozen_string = if cx.target_ruby_version() >= 3.0 {
        uninterpolated_string(node) || uninterpolated_heredoc(node)
    } else {
        matches!(node.ty(), NodeType::Str | NodeType::Dstr)
    };
    frozen_string && frozen_string_literals_enabled(cx)
}
