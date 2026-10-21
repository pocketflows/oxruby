//! Port of `lib/rubocop/cop/layout/first_hash_element_indentation.rb`.

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
pub struct FirstHashElementIndentation {
    ignored: IgnoredNodes,
}

impl Cop for FirstHashElementIndentation {
    fn name(&self) -> &'static str {
        "Layout/FirstHashElementIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash, NodeType::Send, NodeType::Csend]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Hash) {
            if node.loc().begin.is_some() {
                self.check(cx, node, None);
            }
            return;
        }
        if enforce_first_argument_with_fixed_indentation(cx) {
            return;
        }
        for (hash_node, left_parenthesis) in each_argument_node(cx.processed_source, node, NodeType::Hash) {
            self.check(cx, hash_node, Some(left_parenthesis));
            self.ignored.ignore(hash_node);
        }
    }
}

fn settings<'a>(cx: &Investigation<'a, '_>) -> Settings<'a> {
    Settings {
        style: style(cx),
        brace_alignment_style: "align_braces",
    }
}

/// `autocorrect(corrector, node)`: a pair whose value starts on a later line
/// than its key is corrected by its first line only.
fn autocorrect(corrector: &mut Corrector, cx: &Investigation<'_, '_>, target: Target<'_>, column_delta: isize) {
    let target = match target {
        Target::Node(node)
            if node
                .pair_value()
                .zip(node.key())
                .is_some_and(|(value, key)| value.first_line() > key.first_line()) =>
        {
            Target::Range(cx.processed_source.line_range(node.first_line()))
        }
        other => other,
    };
    alignment_corrector::correct(corrector, cx.processed_source, cx.config, Some(target), column_delta, false);
}

impl FirstHashElementIndentation {
    fn check<'a>(&self, cx: &mut Investigation<'a, '_>, hash_node: Node<'a>, left_parenthesis: Option<Range>) {
        if self.ignored.contains(hash_node) {
            return;
        }
        let ps = cx.processed_source;
        let left_brace = hash_node.loc().begin.unwrap();
        let first_pair = hash_node.pairs().first().copied();
        if let Some(first_pair) = first_pair {
            if first_pair.first_line() == ps.line_of(left_brace.begin_pos) {
                return;
            }
            let offset = if separator_style(cx, first_pair) {
                let key_lengths: Vec<isize> = hash_node
                    .hash_keys()
                    .iter()
                    .map(|key| key.source().chars().count() as isize)
                    .collect();
                key_lengths.iter().max().unwrap() - key_lengths[0]
            } else {
                0
            };
            check_first(
                cx,
                settings(cx),
                first_pair,
                left_brace,
                left_parenthesis,
                offset,
                &message,
                &|corrector, cx, node, delta| autocorrect(corrector, cx, Target::Node(node), delta),
            );
        }
        check_right_brace(cx, hash_node.loc().end.unwrap(), first_pair, left_brace, left_parenthesis);
    }
}

fn check_right_brace(
    cx: &mut Investigation<'_, '_>,
    right_brace: Range,
    first_pair: Option<Node<'_>>,
    left_brace: Range,
    left_parenthesis: Option<Range>,
) {
    let ps = cx.processed_source;
    let line = ps.source_line(ps.line_of(right_brace.begin_pos));
    let before: String = line.chars().take(ps.column_of(right_brace.begin_pos)).collect();
    if regex!(r"\S").find(&before).is_some() {
        return;
    }
    let (expected_column, indent_base_type) =
        indent_base(ps, settings(cx), left_brace, first_pair, left_parenthesis);
    let column_delta = expected_column as isize - ps.column_of(right_brace.begin_pos) as isize;
    if column_delta == 0 {
        return;
    }
    let msg = message_for_right_brace(indent_base_type);
    cx.add_offense_cx(right_brace, msg, |corrector, cx| {
        autocorrect(corrector, cx, Target::Range(right_brace), column_delta);
    });
}

fn separator_style(cx: &Investigation<'_, '_>, first_pair: Node<'_>) -> bool {
    let colon = first_pair.loc_is(first_pair.loc().operator, ":");
    let key = if colon {
        "EnforcedColonStyle"
    } else {
        "EnforcedHashRocketStyle"
    };
    cx.config
        .for_cop("Layout/HashAlignment")
        .get(key)
        .and_then(Value::as_str)
        == Some("separator")
}

fn base_description(indent_base_type: IndentBase) -> &'static str {
    match indent_base_type {
        IndentBase::LeftBraceOrBracket => "the position of the opening brace",
        IndentBase::FirstColumnAfterLeftParenthesis => {
            "the first position after the preceding left parenthesis"
        }
        IndentBase::ParentHashKey => "the parent hash key",
        IndentBase::StartOfLine => "the start of the line where the left curly brace is",
    }
}

fn message(cx: &Investigation<'_, '_>, indent_base_type: IndentBase) -> String {
    format!(
        "Use {} spaces for indentation in a hash, relative to {}.",
        configured_indentation_width(cx),
        base_description(indent_base_type)
    )
}

fn message_for_right_brace(indent_base_type: IndentBase) -> &'static str {
    match indent_base_type {
        IndentBase::LeftBraceOrBracket => "Indent the right brace the same as the left brace.",
        IndentBase::FirstColumnAfterLeftParenthesis => {
            "Indent the right brace the same as the first position after the preceding left parenthesis."
        }
        IndentBase::ParentHashKey => "Indent the right brace the same as the parent hash key.",
        IndentBase::StartOfLine => {
            "Indent the right brace the same as the start of the line where the left brace is."
        }
    }
}

fn enforce_first_argument_with_fixed_indentation(cx: &Investigation<'_, '_>) -> bool {
    cx.config
        .for_enabled_cop("Layout/ArgumentAlignment")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        == Some("with_fixed_indentation")
}
