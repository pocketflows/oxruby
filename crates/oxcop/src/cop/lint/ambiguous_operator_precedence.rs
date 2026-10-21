//! Port of `lib/rubocop/cop/lint/ambiguous_operator_precedence.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};

const PRECEDENCE: &[&[&str]] = &[
    &["**"],
    &["*", "/", "%"],
    &["+", "-"],
    &["<<", ">>"],
    &["&"],
    &["|", "^"],
    &["&&"],
    &["||"],
];
const RESTRICT_ON_SEND: &[&str] = &[
    "**", "*", "/", "%", "+", "-", "<<", ">>", "&", "|", "^", "&&", "||",
];
const MSG: &str = "Wrap expressions with varying precedence with parentheses to avoid ambiguity.";

#[derive(Default)]
pub struct AmbiguousOperatorPrecedence;

impl Cop for AmbiguousOperatorPrecedence {
    fn name(&self) -> &'static str {
        "Lint/AmbiguousOperatorPrecedence"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::And, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        RESTRICT_ON_SEND
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::And) {
            on_and(cx, node);
        } else {
            on_send(cx, node);
        }
    }
}

fn on_and<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(parent) = node.parent() else {
        return;
    };
    // if the `and` is in a `begin`, it's parenthesized already
    if parent.is(NodeType::Begin) {
        return;
    }
    if !parent.is(NodeType::Or) {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
        autocorrect(corrector, node);
    });
}

fn on_send<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.is_parenthesized() {
        return;
    }
    let Some(parent) = node.parent() else {
        return;
    };
    if !is_operator(parent) {
        return;
    }
    if !greater_precedence(node, parent) {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
        autocorrect(corrector, node);
    });
}

fn precedence(node: Node<'_>) -> Option<usize> {
    let name = operator_name(node)?;
    PRECEDENCE.iter().position(|operators| operators.contains(&name))
}

fn is_operator(node: Node<'_>) -> bool {
    (node.is_send() && node.method_name().is_some_and(|m| RESTRICT_ON_SEND.contains(&m)))
        || node.is_operator_keyword()
}

fn greater_precedence(node1: Node<'_>, node2: Node<'_>) -> bool {
    let (Some(node1_precedence), Some(node2_precedence)) = (precedence(node1), precedence(node2))
    else {
        return false;
    };
    node2_precedence > node1_precedence
}

fn operator_name<'a>(node: Node<'a>) -> Option<&'a str> {
    if node.is_send() {
        node.method_name()
    } else {
        node.operator()
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    corrector.wrap(node.source_range(), "(", ")");
}
