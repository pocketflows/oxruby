//! Port of `lib/rubocop/cop/lint/symbol_conversion.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::symbol_help::hash_key;
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SymbolConversion {
    ignored: IgnoredNodes,
}

impl Cop for SymbolConversion {
    fn name(&self) -> &'static str {
        "Lint/SymbolConversion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Sym, NodeType::Hash]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["to_sym", "intern"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Send => on_send(cx, node),
            NodeType::Sym => self.on_sym(cx, node),
            _ => self.on_hash(cx, node),
        }
    }
}

fn on_send(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(receiver) = node.receiver() else {
        return;
    };
    let Some(correction) = symbol_conversion_correction(receiver) else {
        return;
    };

    register_offense(cx, node, &correction, None);
}

fn symbol_conversion_correction(receiver: Node<'_>) -> Option<String> {
    match receiver.ty() {
        NodeType::Str => receiver.str_content().map(symbol_inspect),
        NodeType::Sym => receiver.sym_value().map(symbol_inspect),
        NodeType::Dstr if !receiver.is_heredoc() => Some(dstr_correction(receiver)),
        _ => None,
    }
}

fn dstr_correction(receiver: Node<'_>) -> String {
    if receiver
        .loc()
        .begin
        .is_some_and(|b| receiver.range_source(b) == "\"")
    {
        let source = receiver.source();
        let inner = source
            .get(1..source.len().saturating_sub(1))
            .unwrap_or_default();
        format!(":\"{inner}\"")
    } else {
        format!(":\"{}\"", dstr_value(receiver))
    }
}

/// `DstrNode#value`.
fn dstr_value(node: Node<'_>) -> String {
    node.child_nodes()
        .map(|child| match child.ty() {
            NodeType::Str => child.str_content().unwrap_or_default().to_owned(),
            NodeType::Dstr => dstr_value(child),
            _ => child.source().to_owned(),
        })
        .collect()
}

impl SymbolConversion {
    fn on_sym(&mut self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        let Some(value) = node.sym_value() else {
            return;
        };
        if self.ignored.contains(node) || properly_quoted(cx, node.source(), &symbol_inspect(value))
        {
            return;
        }
        if in_alias(node) {
            return;
        }
        if in_percent_literal_array(node) {
            return;
        }
        if hash_key(node) {
            correct_hash_key(cx, node);
            return;
        }

        register_offense(cx, node, &symbol_inspect(value), None);
    }

    fn on_hash(&mut self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        if style(cx) != "consistent" {
            return;
        }

        let keys: Vec<Node<'_>> = node
            .pairs()
            .into_iter()
            .filter_map(|pair| pair.key())
            .filter(|key| key.is(NodeType::Sym))
            .collect();

        if keys.iter().any(|key| requires_quotes(*key)) {
            self.correct_inconsistent_hash_keys(cx, &keys);
        } else {
            for key in keys {
                correct_hash_key(cx, key);
            }
        }
    }

    fn correct_inconsistent_hash_keys(&mut self, cx: &mut Investigation<'_, '_>, keys: &[Node<'_>]) {
        for &key in keys {
            self.ignored.ignore(key);
            if requires_quotes(key) {
                continue;
            }
            let value = key.sym_value().unwrap_or_default();
            let correction = format!("\"{value}\"");
            if properly_quoted(cx, key.source(), &correction) {
                continue;
            }

            let message = format!(
                "Symbol hash key should be quoted for consistency; use `{correction}:` instead."
            );
            register_offense(cx, key, &correction, Some(&message));
        }
    }
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    correction: &str,
    message: Option<&str>,
) {
    let default_message;
    let message = match message {
        Some(message) => message,
        None => {
            default_message =
                format!("Unnecessary symbol conversion; use `{correction}` instead.");
            &default_message
        }
    };
    cx.add_offense_with_correction(node.source_range(), message, |corrector| {
        corrector.replace(node.source_range(), correction);
    });
}

fn properly_quoted(cx: &Investigation<'_, '_>, source: &str, value: &str) -> bool {
    if style(cx) == "strict" && (!source.contains(['\'', '"']) || value.ends_with('=')) {
        return true;
    }
    source == value || source.replace('"', "\\\"").replace('\'', "\"") == value
}

fn requires_quotes(sym_node: Node<'_>) -> bool {
    let inspected = symbol_inspect(sym_node.sym_value().unwrap_or_default());
    crate::regex!(r#"^:".*?"|=$"#).find(&inspected).is_some()
}

fn in_alias(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is(NodeType::Alias))
}

fn in_percent_literal_array(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::Array) && p.is_percent_literal())
}

fn correct_hash_key(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let value = node.sym_value().unwrap_or_default();
    if !value
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return;
    }

    let colon = node.parent().is_some_and(|p| p.is_colon());
    let mut correction = symbol_inspect(value);
    if colon {
        correction = correction
            .strip_prefix(':')
            .unwrap_or(&correction)
            .to_owned();
    }
    if properly_quoted(cx, node.source(), &correction) {
        return;
    }

    let shown = if colon {
        format!("{correction}:")
    } else {
        correction.clone()
    };
    let message = format!("Unnecessary symbol conversion; use `{shown}` instead.");
    register_offense(cx, node, &correction, Some(&message));
}

/// Ruby's `Symbol#inspect`.
pub fn symbol_inspect(name: &str) -> String {
    if simple_symbol(name) {
        format!(":{name}")
    } else {
        format!(":{}", ruby_inspect(name))
    }
}

const OPERATORS: &[&str] = &[
    "[]", "[]=", "**", "!", "!=", "!~", "+", "-", "*", "/", "%", "==", "===", "=~", "<=>", "<",
    "<=", ">", ">=", "<<", ">>", "&", "|", "^", "~", "+@", "-@", "`",
];

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || !c.is_ascii()
}

fn plain_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(is_ident_start) && chars.all(is_ident_char)
}

fn simple_symbol(name: &str) -> bool {
    if OPERATORS.contains(&name) {
        return true;
    }
    if let Some(rest) = name.strip_prefix('$') {
        if rest.chars().count() == 1 && "~*$?!@/\\;,.=:<>\"&`'+0".contains(rest) {
            return true;
        }
        if let Some(opt) = rest.strip_prefix('-') {
            return opt.chars().count() == 1 && opt.chars().all(is_ident_char);
        }
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        return plain_identifier(rest);
    }
    if let Some(rest) = name.strip_prefix("@@") {
        return plain_identifier(rest);
    }
    if let Some(rest) = name.strip_prefix('@') {
        return plain_identifier(rest);
    }
    let base = name
        .strip_suffix('?')
        .or_else(|| name.strip_suffix('!'))
        .or_else(|| name.strip_suffix('='))
        .unwrap_or(name);
    plain_identifier(base)
}
