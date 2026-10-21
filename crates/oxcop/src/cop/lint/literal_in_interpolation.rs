//! Port of `lib/rubocop/cop/lint/literal_in_interpolation.rb`.

use crate::ast::lexer::ruby_float_to_s;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::interpolation::{NODE_TYPES, interpolations};
use crate::cop::mixin::range_help::{contents_range, range_between};
use crate::cop::mixin::util::{ruby_inspect, symbol_inspect};
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Literal interpolation detected.";
const COMPOSITE: &[NodeType] = &[
    NodeType::Array,
    NodeType::Hash,
    NodeType::Pair,
    NodeType::Irange,
    NodeType::Erange,
];

#[derive(Default)]
pub struct LiteralInInterpolation;

impl Cop for LiteralInInterpolation {
    fn name(&self) -> &'static str {
        "Lint/LiteralInInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for begin_node in interpolations(node) {
            on_interpolation(cx, begin_node);
        }
    }
}

fn on_interpolation<'a>(cx: &mut Investigation<'a, '_>, begin_node: Node<'a>) {
    let Some(final_node) = begin_node.children().last().and_then(Child::as_node) else {
        return;
    };
    let final_node = begin_node.ast().node(final_node);
    if !offending(cx, final_node) {
        return;
    }
    let expanded_value = autocorrected_value(cx, final_node);
    let expanded_value = handle_special_regexp_chars(begin_node, expanded_value);
    if in_array_percent_literal(begin_node)
        && regex!(r"\s|\A\z").find(&expanded_value).is_some()
    {
        return;
    }
    cx.add_offense_with_correction(final_node.source_range(), MSG, |corrector| {
        // nested, fixed in next iteration
        if final_node.is(NodeType::Dstr) {
            return;
        }
        let replacement = if final_node.is(NodeType::Str) && !valid_encoding(final_node) {
            strip_quotes(final_node.source()).to_owned()
        } else {
            expanded_value
        };
        corrector.replace(final_node.parent().unwrap().source_range(), replacement);
    });
}

fn valid_encoding(node: Node<'_>) -> bool {
    !matches!(node.value(), Some(Child::Bytes(_)))
}

fn strip_quotes(source: &str) -> &str {
    let s = source.strip_prefix('"').unwrap_or(source);
    s.strip_suffix('"').unwrap_or(s)
}

fn gsub_quote(s: &str) -> String {
    s.replace('"', "\\\"")
}

fn offending(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !special_keyword(node)
        && prints_as_self(node)
        && !(space_literal(node) && ends_heredoc_line(cx, node))
        && !array_in_regexp(node)
}

fn special_keyword(node: Node<'_>) -> bool {
    (node.is(NodeType::Str) && node.loc().begin.is_none()) || node.source() == "__LINE__"
}

fn array_in_regexp(node: Node<'_>) -> bool {
    let grandparent = node.parent().and_then(|p| p.parent());
    node.is(NodeType::Array) && grandparent.is_some_and(|g| g.is(NodeType::Regexp))
}

fn autocorrected_value(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    match node.ty() {
        NodeType::Int => int_to_s(node),
        NodeType::Float => float_to_s(node),
        NodeType::Str => autocorrected_value_for_string(node),
        NodeType::Sym => autocorrected_value_for_symbol(cx, node),
        NodeType::Array => autocorrected_value_for_array(cx, node),
        NodeType::Hash => autocorrected_value_for_hash(cx, node),
        NodeType::Nil => String::new(),
        _ => gsub_quote(node.source()),
    }
}

fn int_to_s(node: Node<'_>) -> String {
    match node.children().last() {
        Some(Child::Int(i)) => i.to_string(),
        Some(Child::BigInt(s)) => s.clone(),
        _ => node.source().to_owned(),
    }
}

fn float_to_s(node: Node<'_>) -> String {
    match node.children().last() {
        Some(Child::Float(f)) => ruby_float_to_s(*f),
        _ => node.source().to_owned(),
    }
}

fn handle_special_regexp_chars(begin_node: Node<'_>, value: String) -> String {
    let Some(parent_node) = begin_node.parent() else {
        return value;
    };
    if !(parent_node.is(NodeType::Regexp)
        && parent_node.loc_is(parent_node.loc().begin, "/")
        && value.contains('/'))
    {
        return value;
    }
    regex!(r"(\\*)/")
        .replace_all(&value, |caps: &onig::Captures<'_>| {
            let backslash_count = caps.at(1).unwrap_or("").len();
            let needed_backslashes = (2 * ((backslash_count + 1) / 4)) + 1;
            format!("{}/", "\\".repeat(needed_backslashes))
        })
}

fn autocorrected_value_for_string(node: Node<'_>) -> String {
    if !valid_encoding(node) {
        return strip_quotes(node.source()).to_owned();
    }
    escape_string_content(node.str_content().unwrap_or_default())
}

fn escape_string_content(string: &str) -> String {
    regex!(r#"[\\"]|#(?=[@{$])"#).replace_all(string, |caps: &onig::Captures<'_>| {
        format!("\\{}", caps.at(0).unwrap_or(""))
    })
}

fn autocorrected_value_for_symbol(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let end_pos = node
        .loc()
        .end
        .map_or(node.source_range().end_pos, |e| e.begin_pos);
    let begin = node.loc().begin.map_or(node.source_range().begin_pos, |b| b.end_pos);
    gsub_quote(cx.source(range_between(begin, end_pos)))
}

fn autocorrected_value_in_hash_for_symbol(node: Node<'_>) -> String {
    escape_string_content(&symbol_inspect(node.sym_value().unwrap_or_default()))
}

fn autocorrected_value_for_array(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    if !node.is_percent_literal() {
        return gsub_quote(node.source());
    }
    // `contents_range(node).source.split.to_s`
    let words: Vec<String> = cx
        .source(contents_range(node))
        .split_ascii_whitespace()
        .map(ruby_inspect)
        .collect();
    gsub_quote(&format!("[{}]", words.join(", ")))
}

fn autocorrected_value_for_hash(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let hash_string: Vec<String> = node
        .child_nodes()
        .map(|child| {
            let key = child
                .key()
                .map(|k| autocorrected_value_in_hash(cx, k))
                .unwrap_or_default();
            let value = child
                .pair_value()
                .map(|v| autocorrected_value_in_hash(cx, v))
                .unwrap_or_default();
            format!("{key}=>{value}")
        })
        .collect();
    format!("{{{}}}", hash_string.join(", "))
}

fn autocorrected_value_in_hash(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    match node.ty() {
        NodeType::Int => int_to_s(node),
        NodeType::Float => float_to_s(node),
        NodeType::Str => escape_string_content(&ruby_inspect(node.str_content().unwrap_or_default())),
        NodeType::Sym => autocorrected_value_in_hash_for_symbol(node),
        NodeType::Array => autocorrected_value_for_array(cx, node),
        NodeType::Hash => autocorrected_value_for_hash(cx, node),
        _ => gsub_quote(node.source()),
    }
}

fn prints_as_self(node: Node<'_>) -> bool {
    node.is_basic_literal()
        || (node.is_any(COMPOSITE)
            && node.children().iter().all(|child| match child {
                Child::Node(id) => prints_as_self(node.ast().node(*id)),
                _ => false,
            }))
}

fn space_literal(node: Node<'_>) -> bool {
    node.is(NodeType::Str)
        && valid_encoding(node)
        && node.str_content().is_some_and(|s| s.trim().is_empty())
}

fn ends_heredoc_line(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let grandparent = node.parent().and_then(|p| p.parent());
    if !grandparent.is_some_and(|g| g.is(NodeType::Dstr) && g.is_heredoc()) {
        return false;
    }
    let Some(line) = cx.processed_source.lines().get(node.last_line() - 1) else {
        return false;
    };
    line.chars().count() == node.last_column() + 1
}

fn in_array_percent_literal(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !parent.is_any(&[NodeType::Dstr, NodeType::Dsym]) {
        return false;
    }
    parent
        .parent()
        .is_some_and(|g| g.is(NodeType::Array) && g.is_percent_literal())
}
