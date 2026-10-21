//! Port of `lib/rubocop/cop/lint/ambiguous_operator.rb`.
//!
//! RuboCop reads `diagnostic.reason == :ambiguous_prefix` from
//! `Prism::Translation::Parser`, which maps Prism's
//! `ambiguous_first_argument_{plus,minus}` and `ambiguous_prefix_{ampersand,
//! star,star_star}` warnings to it. The Rust Prism API only exposes a
//! warning's message, so the warnings are recognized by their message.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::add_parentheses;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

/// Prism's warning messages for `:ambiguous_prefix`, with the prefix.
const AMBIGUOUS_PREFIX_WARNINGS: &[(&str, &str)] = &[
    (
        "ambiguous first argument; put parentheses or a space even after `+` operator",
        "+",
    ),
    (
        "ambiguous first argument; put parentheses or a space even after `-` operator",
        "-",
    ),
    ("ambiguous `&` has been interpreted as an argument prefix", "&"),
    ("ambiguous `*` has been interpreted as an argument prefix", "*"),
    ("ambiguous `**` has been interpreted as an argument prefix", "**"),
];

/// `AMBIGUITIES`: operator, actual, possible.
const AMBIGUITIES: &[(&str, &str, &str)] = &[
    ("+", "positive number", "an addition"),
    ("-", "negative number", "a subtraction"),
    ("*", "splat", "a multiplication"),
    ("&", "block", "a binary AND"),
    ("**", "keyword splat", "an exponent"),
];

#[derive(Default)]
pub struct AmbiguousOperator;

impl Cop for AmbiguousOperator {
    fn name(&self) -> &'static str {
        "Lint/AmbiguousOperator"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Naming/BlockForwarding"]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        let diagnostics: Vec<(Range, &str)> = ps
            .parse_result()
            .warnings()
            .filter_map(|warning| {
                let message = String::from_utf8_lossy(warning.message()).into_owned();
                let (_, prefix) = AMBIGUOUS_PREFIX_WARNINGS
                    .iter()
                    .find(|(text, _)| *text == message)?;
                Some((Range::from(&warning.location()), *prefix))
            })
            .collect();
        let Some(ast) = ps.ast() else {
            return;
        };
        for (location, prefix) in diagnostics {
            let Some(offense_node) = find_offense_node_by(ast, location, prefix) else {
                continue;
            };
            let message = message(cx.source(location));
            let src = ps.buffer_source();
            cx.add_offense_with_correction(location, &message, |corrector| {
                add_parentheses(src, offense_node, corrector);
            });
        }
    }
}

fn find_offense_node_by<'a>(ast: Node<'a>, location: Range, prefix: &str) -> Option<Node<'a>> {
    for node in ast.each_node() {
        if !node.is_any(&[NodeType::Splat, NodeType::BlockPass, NodeType::Kwsplat]) {
            continue;
        }
        if !offense_position(node, location) {
            continue;
        }
        if let Some(offense_node) = offense_node(node) {
            return Some(offense_node);
        }
    }
    ast.each_node().filter(|n| n.is(NodeType::Send)).find(|send_node| {
        send_node.first_argument().is_some_and(|first_argument| {
            offense_position(first_argument, location) && first_argument.source().starts_with(prefix)
        })
    })
}

fn message(operator: &str) -> String {
    let (operator, actual, possible) = AMBIGUITIES
        .iter()
        .find(|(op, _, _)| *op == operator)
        .copied()
        .unwrap_or((operator, "", ""));
    format!(
        "Ambiguous {actual} operator. Parenthesize the method arguments if it's surely a {actual} operator, or add a whitespace to the right of the `{operator}` if it should be {possible}."
    )
}

fn offense_position(node: Node<'_>, location: Range) -> bool {
    node.source_range().begin_pos == location.begin_pos
}

fn offense_node(node: Node<'_>) -> Option<Node<'_>> {
    match node.ty() {
        NodeType::Splat | NodeType::BlockPass => node.parent(),
        NodeType::Kwsplat => node.parent().and_then(|p| p.parent()),
        _ => None,
    }
}
