//! Port of `lib/rubocop/cop/style/string_literals.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::string_literal_corrector;
use crate::cop::base::raise;
use crate::cop::mixin::configurable_enforced_style::{
    correct_style_detected, opposite_style_detected, style, supported_styles,
};
use crate::cop::mixin::string_help::{inside_interpolation, StringHelp, NODE_TYPES};
use crate::cop::mixin::string_literals_help::wrong_quotes;
use crate::cop::mixin::util::double_quotes_required;
use crate::cop::{Cop, Investigation};

const MSG_INCONSISTENT: &str = "Inconsistent quote style.";

#[derive(Default)]
pub struct StringLiterals {
    string_help: StringHelp,
}

impl Cop for StringLiterals {
    fn name(&self) -> &'static str {
        "Style/StringLiterals"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str, NodeType::Regexp, NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Dstr) {
            self.on_dstr(cx, node);
            return;
        }
        debug_assert!(NODE_TYPES.contains(&node.ty()));
        let Some(node) = self.string_help.candidate(node) else {
            return;
        };
        check_style(cx);
        if wrong_quotes(cx, node.source()) && !inside_interpolation(node) {
            if register_offense(cx, node, None) {
                opposite_style_detected(cx, "EnforcedStyle");
            }
        } else {
            correct_style_detected(cx, "EnforcedStyle");
        }
    }
}

impl StringLiterals {
    fn on_dstr<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !cx.cop_config_bool("ConsistentQuotesInMultiline", false) || node.is_heredoc() {
            return;
        }
        let children: Vec<Node<'a>> = node.child_nodes().collect();
        if children.len() != node.children().len()
            || !children
                .iter()
                .all(|n| n.is_any(&[NodeType::Str, NodeType::Dstr]))
        {
            return;
        }
        let quote_styles = detect_quote_styles(node, &children);
        if quote_styles.len() > 1 {
            register_offense(cx, node, Some(MSG_INCONSISTENT));

        } else if let Some(quote) = quote_styles.first() {
            check_multiline_quote_style(cx, node, &children, quote);
        }
        self.string_help.ignored.ignore(node);
    }
}

/// `style` raises for a style that isn't supported.
pub(crate) fn check_style(cx: &Investigation<'_, '_>) {
    let style = style(cx);
    if !supported_styles(cx, "EnforcedStyle").contains(&style) {
        raise("RuntimeError", format!("Unknown style {style} selected!"));
    }
}

/// Returns whether the correction block ran (the offense was registered on
/// an enabled line).
fn register_offense(cx: &mut Investigation<'_, '_>, node: Node<'_>, message: Option<&str>) -> bool {
    let style = style(cx);
    let message = message.unwrap_or(if style == "single_quotes" {
        "Prefer single-quoted strings when you don't need string interpolation or special symbols."
    } else {
        "Prefer double-quoted strings unless you need single quotes to avoid extra backslashes for escaping."
    });
    let mut ran = false;
    cx.add_offense_with_correction(node.source_range(), message, |corrector| {
        ran = true;
        string_literal_corrector::correct(corrector, node, style);
    });
    ran
}

fn detect_quote_styles(node: Node<'_>, children: &[Node<'_>]) -> Vec<String> {
    let styles: Vec<Option<&str>> = children
        .iter()
        .map(|c| c.loc().begin.map(|b| c.range_source(b)))
        .collect();
    if styles.iter().all(Option::is_none) {
        return node
            .loc()
            .begin
            .map(|b| vec![node.range_source(b).to_owned()])
            .unwrap_or_default();
    }
    let mut unique: Vec<String> = Vec::new();
    for style in styles {
        // `nil.source` would raise; a child without quotes can't be mixed in.
        let style = style.unwrap_or_default().to_owned();
        if !unique.contains(&style) {
            unique.push(style);
        }
    }
    unique
}

fn check_multiline_quote_style(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    children: &[Node<'_>],
    quote: &str,
) {
    let style = style(cx);
    if quote == "'" && style == "double_quotes" {
        if children.iter().all(|c| wrong_quotes(cx, c.source())) {
            register_offense(cx, node, None);
        }
    } else if quote == "\"" && style == "single_quotes" && !accept_child_double_quotes(children) {
        register_offense(cx, node, None);
    }
}

fn accept_child_double_quotes(nodes: &[Node<'_>]) -> bool {
    nodes
        .iter()
        .any(|n| n.is(NodeType::Dstr) || double_quotes_required(n.source()))
}
