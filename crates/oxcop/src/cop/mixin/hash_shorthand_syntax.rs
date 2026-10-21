//! Port of `lib/rubocop/cop/mixin/hash_shorthand_syntax.rb`. The including
//! cop supplies `hash_rockets_enforced?(hash_node)` (its `style` and
//! `force_hash_rockets?`).

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::parentheses;
use crate::processed_source::Range;

pub const OMIT_HASH_VALUE_MSG: &str = "Omit the hash value.";
pub const EXPLICIT_HASH_VALUE_MSG: &str = "Include the hash value.";
pub const DO_NOT_MIX_OMIT_VALUE_MSG: &str =
    "Do not mix explicit and implicit hash values. Omit the hash value.";
pub const DO_NOT_MIX_EXPLICIT_VALUE_MSG: &str =
    "Do not mix explicit and implicit hash values. Include the hash value.";

pub type HashRocketsEnforced = fn(&Investigation<'_, '_>, Node<'_>) -> bool;

/// `DefNode`: the call whose arguments get parentheses.
struct DefNode<'a> {
    node: Node<'a>,
}

impl DefNode<'_> {
    fn selector(&self) -> Range {
        self.node
            .loc()
            .selector
            .or(self.node.loc().keyword)
            .unwrap()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Omitted,
    Needed,
    Omittable,
}

/// `hash_value_type_breakdown`: `group_by`, keys in first-seen order.
type Breakdown<'a> = Vec<(ValueType, Vec<Node<'a>>)>;

fn group<'a, 'b>(breakdown: &'b Breakdown<'a>, ty: ValueType) -> Option<&'b Vec<Node<'a>>> {
    breakdown.iter().find(|(t, _)| *t == ty).map(|(_, v)| v)
}

pub fn on_hash_for_mixed_shorthand(
    cx: &mut Investigation<'_, '_>,
    hash_node: Node<'_>,
    hash_rockets_enforced: HashRocketsEnforced,
) {
    if ignore_mixed_hash_shorthand_syntax(cx, hash_node, hash_rockets_enforced) {
        return;
    }
    let breakdown = breakdown_value_types_of_hash(hash_node);
    if breakdown.len() > 1 {
        mixed_shorthand_syntax_check(cx, &breakdown);
    } else {
        no_mixed_shorthand_syntax_check(cx, &breakdown);
    }
}

pub fn on_pair(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    hash_rockets_enforced: HashRocketsEnforced,
) {
    if ignore_hash_shorthand_syntax(cx, node, hash_rockets_enforced) {
        return;
    }
    let hash_key_source = node.key().unwrap().source();
    let (message, replacement) = if enforced_shorthand_syntax(cx) == "always" {
        if node.is_value_omission() || require_hash_value(hash_key_source, node) {
            return;
        }
        (OMIT_HASH_VALUE_MSG, format!("{hash_key_source}:"))
    } else {
        if !node.is_value_omission() {
            return;
        }
        (
            EXPLICIT_HASH_VALUE_MSG,
            format!("{hash_key_source}: {hash_key_source}"),
        )
    };
    register_offense(cx, node, message, &replacement);
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    message: &str,
    replacement: &str,
) {
    let value = node.pair_value().unwrap();
    cx.add_offense_with_correction(value.source_range(), message, |corrector| {
        corrector.replace(node.source_range(), replacement);
        let Some(def_node) = def_node_that_require_parentheses(node) else {
            return;
        };
        let last_argument = def_node.node.last_argument();
        let Some(last_argument) = last_argument.filter(|l| l.is(NodeType::Hash)) else {
            corrector.replace(node.source_range(), replacement);
            return;
        };
        let white_spaces = range_between(
            def_node.selector().end_pos,
            def_node
                .node
                .first_argument()
                .unwrap()
                .source_range()
                .begin_pos,
        );
        if node.parent().is_some_and(|p| p.has_braces()) {
            return;
        }
        corrector.replace(white_spaces, "(");
        if last_argument.pairs().last() == Some(&node) {
            corrector.insert_after(last_argument.source_range(), ")");
        }
    });
}

fn ignore_mixed_hash_shorthand_syntax(
    cx: &Investigation<'_, '_>,
    hash_node: Node<'_>,
    hash_rockets_enforced: HashRocketsEnforced,
) -> bool {
    cx.target_ruby_version() <= 3.0
        || !matches!(
            enforced_shorthand_syntax(cx),
            "consistent" | "either_consistent"
        )
        || !hash_node.is(NodeType::Hash)
        || hash_rockets_enforced(cx, hash_node)
}

fn ignore_hash_shorthand_syntax(
    cx: &Investigation<'_, '_>,
    pair_node: Node<'_>,
    hash_rockets_enforced: HashRocketsEnforced,
) -> bool {
    let shorthand = enforced_shorthand_syntax(cx);
    cx.target_ruby_version() <= 3.0
        || shorthand == "either"
        || matches!(shorthand, "consistent" | "either_consistent")
        || !pair_node.parent().is_some_and(|p| p.is(NodeType::Hash))
        || hash_rockets_enforced(cx, pair_node.parent().unwrap())
}

fn enforced_shorthand_syntax<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    cx.cop_config_str("EnforcedShorthandSyntax")
        .unwrap_or("always")
}

fn require_hash_value(hash_key_source: &str, node: Node<'_>) -> bool {
    if !node.key().is_some_and(|k| k.is(NodeType::Sym))
        || require_hash_value_for_around_hash_literal(node)
    {
        return true;
    }
    let hash_value = node.pair_value().unwrap();
    if !hash_value.is_any(&[NodeType::Send, NodeType::Lvar]) {
        return true;
    }
    hash_key_source != hash_value.source()
        || hash_key_source.ends_with('!')
        || hash_key_source.ends_with('?')
}

fn require_hash_value_for_around_hash_literal(node: Node<'_>) -> bool {
    let Some(method_dispatch_node) = find_ancestor_method_dispatch_node(node) else {
        return false;
    };
    let parent = node.parent().unwrap();
    !parent.has_braces()
        && !use_element_of_hash_literal_as_receiver(method_dispatch_node, parent)
        && use_modifier_form_without_parenthesized_method_call(method_dispatch_node)
}

fn is_assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

fn def_node_that_require_parentheses(node: Node<'_>) -> Option<DefNode<'_>> {
    let last_pair = node.parent()?.pairs().into_iter().last()?;
    if last_pair.key()?.source() != last_pair.pair_value()?.source() {
        return None;
    }
    let dispatch_node = find_ancestor_method_dispatch_node(node)?;
    if dispatch_node.is_assignment_method() || dispatch_node.is_parenthesized() {
        return None;
    }
    if dispatch_node.parent().is_some_and(parentheses) {
        return None;
    }
    if last_expression(dispatch_node) && !requires_parentheses_context(dispatch_node) {
        return None;
    }
    let def_node = node
        .ancestors()
        .find(|a| a.is_call() || a.is_any(&[NodeType::Super, NodeType::Yield]));
    match def_node {
        Some(def_node) if def_node.arguments().is_empty() => None,
        Some(def_node) => Some(DefNode { node: def_node }),
        None => None,
    }
}

fn find_ancestor_method_dispatch_node(node: Node<'_>) -> Option<Node<'_>> {
    let ancestor = node.parent()?.parent()?;
    if !(ancestor.is_call() || ancestor.is_any(&[NodeType::Super, NodeType::Yield])) {
        return None;
    }
    if ancestor.is_method("[]") || ancestor.is_method("[]=") {
        return None;
    }
    Some(ancestor)
}

/// `{value:}.do_something` is a valid syntax.
fn use_element_of_hash_literal_as_receiver(ancestor: Node<'_>, parent: Node<'_>) -> bool {
    ancestor.is(NodeType::Send) && ancestor.receiver() == Some(parent)
}

fn use_modifier_form_without_parenthesized_method_call(ancestor: Node<'_>) -> bool {
    if ancestor.is_parenthesized() {
        return false;
    }
    ancestor.ancestors().any(|n| n.is_modifier_form())
}

fn last_expression(node: Node<'_>) -> bool {
    if node.right_sibling().is_some() {
        return false;
    }
    let Some(assignment_node) = node.ancestors().find(|a| is_assignment(*a)) else {
        return true;
    };
    if let Some(parent) = assignment_node.parent()
        && is_assignment(parent)
    {
        return last_expression(parent);
    }
    assignment_node.right_sibling().is_none()
}

fn requires_parentheses_context(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        parent.is_call()
            || parent.is_any(&[
                NodeType::If,
                NodeType::Super,
                NodeType::Until,
                NodeType::While,
                NodeType::Yield,
            ])
    })
}

fn breakdown_value_types_of_hash(hash_node: Node<'_>) -> Breakdown<'_> {
    let mut breakdown: Breakdown<'_> = Vec::new();
    for pair_node in hash_node.pairs() {
        let ty = if pair_node.is_value_omission() {
            ValueType::Omitted
        } else if require_hash_value(pair_node.key().unwrap().source(), pair_node) {
            ValueType::Needed
        } else {
            ValueType::Omittable
        };
        match breakdown.iter_mut().find(|(t, _)| *t == ty) {
            Some((_, nodes)) => nodes.push(pair_node),
            None => breakdown.push((ty, vec![pair_node])),
        }
    }
    breakdown
}

fn hash_with_values_that_cant_be_omitted(breakdown: &Breakdown<'_>) -> bool {
    group(breakdown, ValueType::Needed).is_some_and(|v| !v.is_empty())
}

fn mixed_shorthand_syntax_check(cx: &mut Investigation<'_, '_>, breakdown: &Breakdown<'_>) {
    if hash_with_values_that_cant_be_omitted(breakdown) {
        for pair_node in group(breakdown, ValueType::Omitted).into_iter().flatten() {
            let key = pair_node.key().unwrap().source();
            register_offense(
                cx,
                *pair_node,
                DO_NOT_MIX_EXPLICIT_VALUE_MSG,
                &format!("{key}: {key}"),
            );
        }
    } else {
        for pair_node in group(breakdown, ValueType::Omittable).into_iter().flatten() {
            let key = pair_node.key().unwrap().source();
            register_offense(
                cx,
                *pair_node,
                DO_NOT_MIX_OMIT_VALUE_MSG,
                &format!("{key}:"),
            );
        }
    }
}

fn no_mixed_shorthand_syntax_check(cx: &mut Investigation<'_, '_>, breakdown: &Breakdown<'_>) {
    if hash_with_values_that_cant_be_omitted(breakdown) {
        return;
    }
    // `ignore_explicit_omissible_hash_shorthand_syntax?`
    if breakdown.len() == 1
        && breakdown[0].0 == ValueType::Omittable
        && enforced_shorthand_syntax(cx) == "either_consistent"
    {
        return;
    }
    for pair_node in group(breakdown, ValueType::Omittable).into_iter().flatten() {
        let key = pair_node.key().unwrap().source();
        register_offense(cx, *pair_node, OMIT_HASH_VALUE_MSG, &format!("{key}:"));
    }
}
