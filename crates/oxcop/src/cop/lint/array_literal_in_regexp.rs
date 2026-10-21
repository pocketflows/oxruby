//! Port of `lib/rubocop/cop/lint/array_literal_in_regexp.rb`.

use crate::ast::lexer::ruby_float_to_s;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::interpolation::{NODE_TYPES, interpolations};
use crate::cop::{Cop, Investigation};

const LITERAL_TYPES: &[NodeType] = &[
    NodeType::Str,
    NodeType::Sym,
    NodeType::Int,
    NodeType::Float,
    NodeType::True,
    NodeType::False,
    NodeType::Nil,
];

const MSG_CHARACTER_CLASS: &str =
    "Use a character class instead of interpolating an array in a regexp.";
const MSG_ALTERNATION: &str = "Use alternation instead of interpolating an array in a regexp.";
const MSG_UNKNOWN: &str =
    "Use alternation or a character class instead of interpolating an array in a regexp.";

#[derive(Default)]
pub struct ArrayLiteralInRegexp;

impl Cop for ArrayLiteralInRegexp {
    fn name(&self) -> &'static str {
        "Lint/ArrayLiteralInRegexp"
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
    let Some(final_node) = begin_node.child_nodes().last() else {
        return;
    };
    if !final_node.is(NodeType::Array) {
        return;
    }
    if !begin_node.parent().is_some_and(|p| p.is(NodeType::Regexp)) {
        return;
    }
    if array_of_literal_values(final_node) {
        register_array_of_literal_values(cx, begin_node, final_node);
    } else {
        cx.add_offense(begin_node.source_range(), MSG_UNKNOWN);
    }
}

fn array_of_literal_values(node: Node<'_>) -> bool {
    node.child_nodes().all(|value| value.is_any(LITERAL_TYPES))
}

fn register_array_of_literal_values<'a>(
    cx: &mut Investigation<'a, '_>,
    begin_node: Node<'a>,
    node: Node<'a>,
) {
    let array_values = array_values(node);
    let (message, replacement) = if character_class(&array_values) {
        (MSG_CHARACTER_CLASS, character_class_for(&array_values))
    } else {
        (MSG_ALTERNATION, alternation_for(&array_values))
    };
    cx.add_offense_with_correction(begin_node.source_range(), message, |corrector| {
        corrector.replace(begin_node.source_range(), replacement);
    });
}

/// `value.respond_to?(:value) ? value.value : value.source`, as `to_s`.
fn array_values(node: Node<'_>) -> Vec<String> {
    node.child_nodes()
        .map(|value| match (value.ty(), value.value()) {
            (NodeType::Str | NodeType::Sym, Some(Child::Str(s) | Child::Sym(s))) => s.clone(),
            (NodeType::Str, Some(Child::Bytes(b))) => String::from_utf8_lossy(b).into_owned(),
            (NodeType::Int, Some(Child::Int(i))) => i.to_string(),
            (NodeType::Int, Some(Child::BigInt(s))) => s.clone(),
            (NodeType::Float, Some(Child::Float(f))) => ruby_float_to_s(*f),
            _ => value.source().to_owned(),
        })
        .collect()
}

fn character_class(values: &[String]) -> bool {
    values.iter().all(|v| v.chars().count() == 1)
}

fn character_class_for(values: &[String]) -> String {
    format!("[{}]", escape_values(values).join(""))
}

fn alternation_for(values: &[String]) -> String {
    format!("(?:{})", escape_values(values).join("|"))
}

fn escape_values(values: &[String]) -> Vec<String> {
    values.iter().map(|v| regexp_escape(v)).collect()
}

/// Ruby's `Regexp.escape`.
fn regexp_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '[' | ']' | '{' | '}' | '(' | ')' | '|' | '-' | '*' | '.' | '\\' | '?' | '+' | '^'
            | '$' | '#' => {
                out.push('\\');
                out.push(c);
            }
            ' ' => out.push_str("\\ "),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\x0c' => out.push_str("\\f"),
            '\x0b' => out.push_str("\\v"),
            c => out.push(c),
        }
    }
    out
}
