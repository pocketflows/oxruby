//! Port of `lib/rubocop/cop/style/quoted_symbols.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style as enforced_style;
use crate::cop::mixin::symbol_help::hash_key;
use crate::cop::mixin::util::{double_quotes_required, ruby_inspect, to_string_literal};
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG_SINGLE: &str =
    "Prefer single-quoted symbols when you don't need string interpolation or special symbols.";
const MSG_DOUBLE: &str =
    "Prefer double-quoted symbols unless you need single quotes to avoid extra backslashes for escaping.";

#[derive(Default)]
pub struct QuotedSymbols;

impl Cop for QuotedSymbols {
    fn name(&self) -> &'static str {
        "Style/QuotedSymbols"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Sym]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if regex!(r#"(?m)\A:?(['"]).*?\1\z"#).find(node.source()).is_none() {
            return;
        }
        let style = style(cx);
        let message = if style == "single_quotes" {
            MSG_SINGLE
        } else {
            MSG_DOUBLE
        };
        if !(wrong_quotes(style, node) || invalid_double_quotes(style, node.source())) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), message, |corrector| {
            let source = node.source();
            let replacement = if hash_key(node) && node.parent().unwrap().is_colon() {
                correct_quotes(style, strip(source, 1))
            } else {
                format!(":{}", correct_quotes(style, strip(source, 2)))
            };
            corrector.replace(node.source_range(), replacement);
        });
    }
}

/// `source[from..-2]`.
fn strip(source: &str, from: usize) -> &str {
    let mut chars = source.char_indices().skip(from);
    let start = chars.next().map_or(source.len(), |(i, _)| i);
    let end = source.char_indices().last().map_or(0, |(i, _)| i);
    if start <= end { &source[start..end] } else { "" }
}

/// `style`, resolving `same_as_string_literals`.
fn style<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    let style = enforced_style(cx);
    if style != "same_as_string_literals" {
        return style;
    }
    let name = "Style/StringLiterals";
    if !cx.config.cop_enabled(name) {
        return "single_quotes";
    }
    cx.config
        .for_cop(name)
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        .unwrap_or("")
}

/// `StringLiteralsHelp#wrong_quotes?`, with the resolved style.
fn wrong_quotes(style: &str, node: Node<'_>) -> bool {
    let src = if hash_key(node) {
        node.source()
    } else {
        strip_first(node.source())
    };
    if src.starts_with('%') || src.starts_with('?') {
        return false;
    }
    if style == "single_quotes" {
        !double_quotes_required(src)
    } else {
        regex!(r#""|\\[^'\\]|\#[@{$]"#).find(src).is_none()
    }
}

fn strip_first(source: &str) -> &str {
    source
        .char_indices()
        .nth(1)
        .map_or("", |(i, _)| &source[i..])
}

fn invalid_double_quotes(style: &str, source: &str) -> bool {
    if style != "double_quotes" {
        return false;
    }
    regex!(r#""|(?<!\\)\\[aAbcdefkMnprsStuUxzZ0-7]|\#[@{$]"#)
        .find(source)
        .is_none()
}

fn correct_quotes(style: &str, str: &str) -> String {
    let correction = if style == "single_quotes" {
        to_string_literal(str)
    } else {
        ruby_inspect(&str.replace("\\'", "'"))
    };
    correction.replace("\\\\", "\\").replace("\\\"", "\"")
}
