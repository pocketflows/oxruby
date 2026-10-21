//! Port of `lib/rubocop/cop/layout/first_array_element_indentation.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::multiline_element_indentation::{
    IndentBase, Settings, check_first, each_argument_node, indent_base,
};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

#[derive(Default)]
pub struct FirstArrayElementIndentation {
    ignored: IgnoredNodes,
}

impl Cop for FirstArrayElementIndentation {
    fn name(&self) -> &'static str {
        "Layout/FirstArrayElementIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array, NodeType::Send, NodeType::Csend]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Array) {
            if node.loc().begin.is_none() || autocorrect_incompatible_with_other_cops(cx, node, None) {
                return;
            }
            self.check(cx, node, None);
            return;
        }
        for (array_node, left_parenthesis) in each_argument_node(cx.processed_source, node, NodeType::Array) {
            if !autocorrect_incompatible_with_other_cops(cx, array_node, Some(left_parenthesis)) {
                self.check(cx, array_node, Some(left_parenthesis));
            }
            self.ignored.ignore(array_node);
        }
    }
}

fn settings<'a>(cx: &Investigation<'a, '_>) -> Settings<'a> {
    Settings {
        style: style(cx),
        brace_alignment_style: "align_brackets",
    }
}

fn autocorrect_incompatible_with_other_cops(
    cx: &Investigation<'_, '_>,
    array_node: Node<'_>,
    left_parenthesis: Option<Range>,
) -> bool {
    if !enforce_first_argument_with_fixed_indentation(cx) {
        return false;
    }
    if style(cx) != "consistent" {
        return true;
    }
    if array_node.child_nodes().count() < 2 {
        return false;
    }
    let (_, indent_base_type) = indent_base(
        cx.processed_source,
        settings(cx),
        array_node.loc().begin.unwrap(),
        array_node.child_nodes().next(),
        left_parenthesis,
    );
    indent_base_type != IndentBase::StartOfLine
}

fn autocorrect(corrector: &mut Corrector, cx: &Investigation<'_, '_>, target: Target<'_>, column_delta: isize) {
    alignment_corrector::correct(corrector, cx.processed_source, cx.config, Some(target), column_delta, false);
}

impl FirstArrayElementIndentation {
    fn check<'a>(&self, cx: &mut Investigation<'a, '_>, array_node: Node<'a>, left_parenthesis: Option<Range>) {
        if self.ignored.contains(array_node) {
            return;
        }
        let ps = cx.processed_source;
        let left_bracket = array_node.loc().begin.unwrap();
        let first_elem = array_node.child_nodes().next();
        if let Some(first_elem) = first_elem {
            if first_elem.first_line() == ps.line_of(left_bracket.begin_pos) {
                return;
            }
            check_first(
                cx,
                settings(cx),
                first_elem,
                left_bracket,
                left_parenthesis,
                0,
                &message,
                &|corrector, cx, node, delta| autocorrect(corrector, cx, Target::Node(node), delta),
            );
        }
        check_right_bracket(cx, array_node.loc().end.unwrap(), first_elem, left_bracket, left_parenthesis);
    }
}

fn check_right_bracket(
    cx: &mut Investigation<'_, '_>,
    right_bracket: Range,
    first_elem: Option<Node<'_>>,
    left_bracket: Range,
    left_parenthesis: Option<Range>,
) {
    let ps = cx.processed_source;
    let line = ps.source_line(ps.line_of(right_bracket.begin_pos));
    let before: String = line.chars().take(ps.column_of(right_bracket.begin_pos)).collect();
    if regex!(r"\S").find(&before).is_some() {
        return;
    }
    let (expected_column, indent_base_type) =
        indent_base(ps, settings(cx), left_bracket, first_elem, left_parenthesis);
    let column_delta = expected_column as isize - ps.column_of(right_bracket.begin_pos) as isize;
    if column_delta == 0 {
        return;
    }
    let msg = message_for_right_bracket(indent_base_type);
    cx.add_offense_cx(right_bracket, msg, |corrector, cx| {
        autocorrect(corrector, cx, Target::Range(right_bracket), column_delta);
    });
}

fn base_description(indent_base_type: IndentBase) -> &'static str {
    match indent_base_type {
        IndentBase::LeftBraceOrBracket => "the position of the opening bracket",
        IndentBase::FirstColumnAfterLeftParenthesis => {
            "the first position after the preceding left parenthesis"
        }
        IndentBase::ParentHashKey => "the parent hash key",
        IndentBase::StartOfLine => "the start of the line where the left square bracket is",
    }
}

fn message(cx: &Investigation<'_, '_>, indent_base_type: IndentBase) -> String {
    format!(
        "Use {} spaces for indentation in an array, relative to {}.",
        configured_indentation_width(cx),
        base_description(indent_base_type)
    )
}

fn message_for_right_bracket(indent_base_type: IndentBase) -> &'static str {
    match indent_base_type {
        IndentBase::LeftBraceOrBracket => "Indent the right bracket the same as the left bracket.",
        IndentBase::FirstColumnAfterLeftParenthesis => {
            "Indent the right bracket the same as the first position after the preceding left parenthesis."
        }
        IndentBase::ParentHashKey => "Indent the right bracket the same as the parent hash key.",
        IndentBase::StartOfLine => {
            "Indent the right bracket the same as the start of the line where the left bracket is."
        }
    }
}

fn enforce_first_argument_with_fixed_indentation(cx: &Investigation<'_, '_>) -> bool {
    cx.config
        .for_enabled_cop("Layout/ArrayAlignment")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        == Some("with_fixed_indentation")
}
