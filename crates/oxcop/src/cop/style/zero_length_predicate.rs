//! Port of `lib/rubocop/cop/style/zero_length_predicate.rb`.

use crate::ast::node_pattern::{Captures, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static ZERO_LENGTH_PREDICATE: NodePattern =
    NodePattern::new("(call (call (...) {:length :size}) :zero?)");

static ZERO_LENGTH_COMPARISON: NodePattern = NodePattern::new(
    "{(call (call (...) ${:length :size}) $:== (int $0))
      (call (int $0) $:== (call (...) ${:length :size}))
      (call (call (...) ${:length :size}) $:<  (int $1))
      (call (int $1) $:> (call (...) ${:length :size}))}",
);

static NONZERO_LENGTH_COMPARISON: NodePattern = NodePattern::new(
    "{(call (call (...) ${:length :size}) ${:> :!=} (int $0))
      (call (int $0) ${:< :!=} (call (...) ${:length :size}))}",
);

static ZERO_LENGTH_NODE: NodePattern = NodePattern::new(
    "{(send $(call _ _) :== (int 0))
      (send (int 0) :== $(call _ _))
      (send $(call _ _) :<  (int 1))
      (send (int 1) :> $(call _ _))}",
);

static OTHER_LENGTH_NODE: NodePattern = NodePattern::new(
    "{(call $(call _ _) _ _)
      (call _ _ $(call _ _))}",
);

static NON_POLYMORPHIC_COLLECTION: NodePattern = NodePattern::new(
    "{(send (send (send (const {nil? cbase} :File) :stat _) ...) ...)
      (send (send (send (const {nil? cbase} {:File :Tempfile :StringIO}) {:new :open} ...) ...) ...)
      (send (send (send (const (const {nil? cbase} :File) :Stat) :new ...) ...) ...)}",
);

#[derive(Default)]
pub struct ZeroLengthPredicate;

impl Cop for ZeroLengthPredicate {
    fn name(&self) -> &'static str {
        "Style/ZeroLengthPredicate"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["size", "length"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(parent) = node.parent() else {
            return;
        };
        check_zero_length_predicate(cx, node, parent);
        check_comparison(cx, parent, &ZERO_LENGTH_COMPARISON, "empty?");
        if node.is_send() {
            check_comparison(cx, parent, &NONZERO_LENGTH_COMPARISON, "!empty?");
        }
    }
}

fn check_zero_length_predicate(cx: &mut Investigation<'_, '_>, node: Node<'_>, parent: Node<'_>) {
    if !ZERO_LENGTH_PREDICATE.is_match(parent) || NON_POLYMORPHIC_COLLECTION.is_match(parent) {
        return;
    }
    let offense = node
        .loc()
        .selector
        .unwrap()
        .join(parent.source_range().end());
    let message = format!("Use `empty?` instead of `{}`.", cx.source(offense));
    cx.add_offense_with_correction(offense, &message, |corrector| {
        corrector.replace(offense, "empty?");
    });
}

fn check_comparison(
    cx: &mut Investigation<'_, '_>,
    parent: Node<'_>,
    pattern: &NodePattern,
    prefer: &str,
) {
    let Some(captures) = pattern.match_node(parent) else {
        return;
    };
    if NON_POLYMORPHIC_COLLECTION.is_match(parent) {
        return;
    }
    let current = format!(
        "{} {} {}",
        capture_source(&captures, 0),
        capture_source(&captures, 1),
        capture_source(&captures, 2)
    );
    let message = format!("Use `{prefer}` instead of `{current}`.");
    cx.add_offense_with_correction(parent.source_range(), &message, |corrector| {
        corrector.replace(parent.source_range(), replacement(parent));
    });
}

fn capture_source(captures: &Captures<'_>, index: usize) -> String {
    match captures[index].val() {
        Some(Val::Value(crate::ast::Child::Int(i))) => i.to_string(),
        Some(val) => val.sym().unwrap_or_default().to_owned(),
        None => String::new(),
    }
}

fn replacement(node: Node<'_>) -> String {
    let length_node = ZERO_LENGTH_NODE
        .match_node(node)
        .and_then(|c| c[0].node());
    if let Some(length_node) = length_node
        && let Some(receiver) = length_node.receiver()
    {
        return format!(
            "{}{}empty?",
            receiver.source(),
            length_node.range_source(length_node.loc().dot.unwrap())
        );
    }
    let other_length_node = OTHER_LENGTH_NODE
        .match_node(node)
        .and_then(|c| c[0].node())
        .unwrap();
    format!(
        "!{}{}empty?",
        other_length_node.receiver().unwrap().source(),
        other_length_node.range_source(other_length_node.loc().dot.unwrap())
    )
}
