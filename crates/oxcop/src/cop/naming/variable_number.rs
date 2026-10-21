//! Port of `lib/rubocop/cop/naming/variable_number.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::{
    alternative_styles, correct_style_detected, style, unexpected_style_detected,
    unrecognized_style_detected,
};
use crate::cop::mixin::configurable_naming::{allowed_identifier, class_emitter_method};
use crate::cop::mixin::configurable_numbering::numbering_format;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct VariableNumber;

impl Cop for VariableNumber {
    fn name(&self) -> &'static str {
        "Naming/VariableNumber"
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[Arg, Lvasgn, Ivasgn, Cvasgn, Gvasgn, Def, Defs, Sym]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Def | NodeType::Defs => {
                let name = node.method_name().unwrap_or("");
                if allowed_identifier(cx, name) || !cx.cop_config_bool("CheckMethodNames", false) {
                    return;
                }
                check_name(cx, node, name, node.loc().name.unwrap());
            }
            NodeType::Sym => {
                // Prism parses a quoted empty hash key (`{ "": value }`) as an
                // empty symbol node.
                let Some(Child::Sym(value)) = node.children().first() else {
                    return;
                };
                if value.is_empty() || allowed_identifier(cx, value) {
                    return;
                }
                if cx.cop_config_bool("CheckSymbols", false) {
                    check_name(cx, node, value, node.source_range());
                }
            }
            _ => {
                let Some(name) = node.name() else {
                    return;
                };
                if allowed_identifier(cx, name) {
                    return;
                }
                check_name(cx, node, name, node.loc().name.unwrap());
            }
        }
    }
}

/// `valid_name?(node, name, given_style = style)`.
fn valid_name(cx: &Investigation<'_, '_>, node: Node<'_>, name: &str, given_style: &str) -> bool {
    numbering_format(given_style).is_some_and(|f| f.find(name).is_some())
        || class_emitter_method(node, name)
        || matches_allowed_pattern(cx, name)
}

/// `ConfigurableFormatting#check_name`.
fn check_name(cx: &mut Investigation<'_, '_>, node: Node<'_>, name: &str, name_range: Range) {
    if valid_name(cx, node, name, style(cx)) {
        correct_style_detected(cx, "EnforcedStyle");
        return;
    }
    let identifier_type = match node.ty() {
        NodeType::Def | NodeType::Defs => "method name",
        NodeType::Sym => "symbol",
        _ => "variable",
    };
    let message = format!("Use {} for {identifier_type} numbers.", style(cx));
    // `add_offense`'s block runs for a new offense on an enabled line.
    let line = cx.processed_source.line_of(name_range.begin_pos);
    let block = !cx.has_offense_at(name_range) && cx.enabled_line(line);
    cx.add_offense(name_range, &message);
    if block {
        report_opposing_styles(cx, node, name);
    }
}

/// `ConfigurableFormatting#report_opposing_styles`.
fn report_opposing_styles(cx: &mut Investigation<'_, '_>, node: Node<'_>, name: &str) {
    for alternative in alternative_styles(cx, "EnforcedStyle") {
        if valid_name(cx, node, name, alternative) {
            unexpected_style_detected(cx, "EnforcedStyle", alternative);
            return;
        }
    }
    unrecognized_style_detected(cx);
}
