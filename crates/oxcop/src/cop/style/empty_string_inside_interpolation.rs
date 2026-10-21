//! Port of `lib/rubocop/cop/style/empty_string_inside_interpolation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::interpolation::{self, interpolations};
use crate::cop::{Cop, Investigation};

const MSG_TRAILING_CONDITIONAL: &str = "Do not use trailing conditionals in string interpolation.";
const MSG_TERNARY: &str = "Do not return empty strings in string interpolation.";

#[derive(Default)]
pub struct EmptyStringInsideInterpolation;

impl Cop for EmptyStringInsideInterpolation {
    fn name(&self) -> &'static str {
        "Style/EmptyStringInsideInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        interpolation::NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for begin_node in interpolations(node) {
            on_interpolation(cx, begin_node);
        }
    }
}

fn on_interpolation(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    for child_node in node.child_nodes().filter(|c| c.is(NodeType::If)) {
        match style(cx) {
            "trailing_conditional" => trailing_conditional_correction(cx, child_node),
            "ternary" => ternary_correction(cx, node, child_node),
            _ => {}
        }
    }
}

fn trailing_conditional_correction(cx: &mut Investigation<'_, '_>, child_node: Node<'_>) {
    if child_node.is_modifier_form() {
        return;
    }
    if empty_branch_outcome(child_node.if_branch()) {
        let outcome = child_node.else_branch().unwrap().source();
        ternary_style_autocorrect(cx, child_node, outcome, "unless");
    }
    if !empty_branch_outcome(child_node.else_branch()) {
        return;
    }
    let outcome = child_node.if_branch().unwrap().source();
    ternary_style_autocorrect(cx, child_node, outcome, "if");
}

fn ternary_correction(cx: &mut Investigation<'_, '_>, node: Node<'_>, child_node: Node<'_>) {
    if !child_node.is_modifier_form() {
        return;
    }
    let if_source = child_node.if_branch().map_or("", |b| b.source());
    let ternary_component = if child_node.is_unless() {
        format!("'' : {if_source}")
    } else {
        format!("{if_source} : ''")
    };
    let condition = child_node.condition().unwrap().source();
    cx.add_offense_with_correction(node.source_range(), MSG_TRAILING_CONDITIONAL, |corrector| {
        corrector.replace(
            node.source_range(),
            format!("#{{{condition} ? {ternary_component}}}"),
        );
    });
}

fn empty_branch_outcome(branch: Option<Node<'_>>) -> bool {
    let Some(branch) = branch else {
        return false;
    };
    branch.is(NodeType::Nil) || branch.str_content().is_some_and(str::is_empty)
}

fn ternary_style_autocorrect(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    outcome: &str,
    condition: &str,
) {
    let replacement = format!(
        "{outcome} {condition} {}",
        node.condition().unwrap().source()
    );
    cx.add_offense_with_correction(node.source_range(), MSG_TERNARY, |corrector| {
        corrector.replace(node.source_range(), replacement);
    });
}
