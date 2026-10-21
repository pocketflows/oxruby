//! Port of `lib/rubocop/cop/style/mutable_constant.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::frozen_string_literal::frozen_string_literal;
use crate::cop::{Cop, Corrector, Investigation};
use crate::magic_comment::MagicComment;

const MSG: &str = "Freeze mutable objects assigned to constants.";

static SPLAT_VALUE: NodePattern = NodePattern::new("(array (splat $_))");
static OPERATION_PRODUCES_IMMUTABLE_OBJECT: NodePattern = NodePattern::new(
    "{
      (const _ _)
      (send (const {nil? cbase} :Struct) :new ...)
      (block (send (const {nil? cbase} :Struct) :new ...) ...)
      (send (const {nil? cbase} :Data) :define ...)
      (block (send (const {nil? cbase} :Data) :define ...) ...)
      (send _ :freeze)
      (send {float int} {:+ :- :* :** :/ :% :<<} _)
      (send _ {:+ :- :* :** :/ :%} {float int})
      (send _ {:== :=== :!= :<= :>= :< :>} _)
      (send (const {nil? cbase} :ENV) :[] _)
      (or (send (const {nil? cbase} :ENV) :[] _) _)
      (send _ {:count :length :size} ...)
      (block (send _ {:count :length :size} ...) ...)
    }",
);
static RANGE_ENCLOSED_IN_PARENTHESES: NodePattern = NodePattern::new("(begin (range _ _))");

#[derive(Default)]
pub struct MutableConstant;

impl Cop for MutableConstant {
    fn name(&self) -> &'static str {
        "Style/MutableConstant"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Casgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let value = match node.expression() {
            Some(value) => value,
            None => {
                let Some(parent) = node.parent().filter(|p| p.is(NodeType::OrAsgn)) else {
                    return;
                };
                match parent.child_nodes().last() {
                    Some(v) => v,
                    None => return,
                }
            }
        };
        let strict = style(cx) == "strict";
        let mut nodes = Vec::new();
        mutable_nodes(cx, value, strict, &mut nodes);
        for node in nodes {
            cx.add_offense_cx(node.source_range(), MSG, |corrector, cx| {
                autocorrect(cx_view(cx), corrector, node);
            });
        }
    }
}

/// What `autocorrect` needs from the investigation (immutable access).
struct View<'c> {
    recursive: bool,
    ruby_version: f64,
    frozen: Box<dyn Fn(Node<'_>) -> bool + 'c>,
    shareable: Box<dyn Fn(Node<'_>) -> bool + 'c>,
}

fn cx_view<'c>(cx: &'c Investigation<'_, '_>) -> View<'c> {
    View {
        recursive: cx.cop_config_bool("Recursive", false),
        ruby_version: cx.target_ruby_version(),
        frozen: Box::new(move |n| frozen_string_literal(cx, n)),
        shareable: Box::new(move |n| shareable_constant_value(cx, n)),
    }
}

fn recursive(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("Recursive", false)
}

fn mutable_nodes<'a>(cx: &Investigation<'_, '_>, value: Node<'a>, strict: bool, out: &mut Vec<Node<'a>>) {
    if recursive(cx) && explicitly_frozen_literal(cx.target_ruby_version(), value) {
        for child in literal_children(value.receiver().unwrap()) {
            mutable_nodes(cx, child, strict, out);
        }
        return;
    }
    let offending = if strict {
        strict_check(cx, value)
    } else {
        literal_check(cx, value)
    };
    if offending {
        out.push(value);
    }
}

fn strict_check(cx: &Investigation<'_, '_>, value: Node<'_>) -> bool {
    !(immutable_literal(cx.target_ruby_version(), value)
        || OPERATION_PRODUCES_IMMUTABLE_OBJECT.is_match(value)
        || frozen_string_literal(cx, value)
        || shareable_constant_value(cx, value))
}

fn literal_check(cx: &Investigation<'_, '_>, value: Node<'_>) -> bool {
    let version = cx.target_ruby_version();
    let mutable_or_unfrozen_range = mutable_literal(version, value)
        || (version <= 2.7 && RANGE_ENCLOSED_IN_PARENTHESES.is_match(value));
    mutable_or_unfrozen_range
        && !frozen_string_literal(cx, value)
        && !shareable_constant_value(cx, value)
}

fn autocorrect(view: View<'_>, corrector: &mut Corrector, node: Node<'_>) {
    autocorrect_node(&view, corrector, node);
}

fn autocorrect_node(view: &View<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let expr = node.source_range();
    if let Some(splat_value) = SPLAT_VALUE.match_node(node).and_then(|c| c[0].node()) {
        if RANGE_ENCLOSED_IN_PARENTHESES.is_match(splat_value) {
            corrector.replace(expr, format!("{}.to_a", splat_value.source()));
        } else {
            corrector.replace(expr, format!("({}).to_a", splat_value.source()));
        }
        corrector.insert_after(expr, ".freeze");
        return;
    }
    if node.is(NodeType::Array) && !node.is_bracketed() {
        corrector.wrap(expr, "[", "]");
    } else if requires_parentheses(node) {
        corrector.wrap(expr, "(", ")");
    }
    corrector.insert_after(expr, ".freeze");
    if view.recursive {
        freeze_nested_literals(view, corrector, node);
    }
}

fn freeze_nested_literals(view: &View<'_>, corrector: &mut Corrector, node: Node<'_>) {
    for child in literal_children(node) {
        if explicitly_frozen_literal(view.ruby_version, child) {
            freeze_nested_literals(view, corrector, child.receiver().unwrap());
        } else if !(view.frozen)(child)
            && !(view.shareable)(child)
            && mutable_literal(view.ruby_version, child)
        {
            autocorrect_node(view, corrector, child);
        }
    }
}

fn literal_children(node: Node<'_>) -> Vec<Node<'_>> {
    match node.ty() {
        NodeType::Array => {
            if node.is_percent_literal() {
                Vec::new()
            } else {
                node.child_nodes().collect()
            }
        }
        NodeType::Hash => node
            .child_nodes()
            .filter(|c| c.is(NodeType::Pair))
            .flat_map(|p| p.child_nodes().collect::<Vec<_>>())
            .collect(),
        _ => Vec::new(),
    }
}

fn explicitly_frozen_literal(version: f64, node: Node<'_>) -> bool {
    node.is_send()
        && node.is_method("freeze")
        && node.receiver().is_some_and(|r| mutable_literal(version, r))
}

fn mutable_literal(version: f64, value: Node<'_>) -> bool {
    !frozen_regexp_or_range_literals(version, value) && value.is_mutable_literal()
}

fn immutable_literal(version: f64, node: Node<'_>) -> bool {
    frozen_regexp_or_range_literals(version, node) || node.is_immutable_literal()
}

fn frozen_regexp_or_range_literals(version: f64, node: Node<'_>) -> bool {
    version >= 3.0 && (node.is(NodeType::Regexp) || node.is_range())
}

fn requires_parentheses(node: Node<'_>) -> bool {
    node.is_range() || (node.is_send() && node.loc().dot.is_none())
}

fn shareable_constant_value(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if cx.target_ruby_version() < 3.0 {
        return false;
    }
    let lines = cx.processed_source.lines();
    let last = node.last_line().min(lines.len());
    let comment = lines[..last]
        .iter()
        .rev()
        .find(|line| MagicComment::parse(line).valid_shareable_constant_value());
    comment.is_some_and(|line| {
        matches!(
            MagicComment::parse(line).shareable_constant_value().as_deref(),
            Some("literal" | "experimental_everything" | "experimental_copy")
        )
    })
}
