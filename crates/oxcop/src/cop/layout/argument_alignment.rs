//! Port of `lib/rubocop/cop/layout/argument_alignment.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{check_alignment, configured_indentation_width, display_column};
use crate::cop::{Cop, Investigation};

const ALIGN_PARAMS_MSG: &str = "Align the arguments of a method call if they span more than one line.";
const FIXED_INDENT_MSG: &str = "Use one level of indentation for arguments following the first line of a multi-line method call.";

/// `Layout::HashAlignment::SEPARATOR_ALIGNMENT_STYLES`.
const SEPARATOR_ALIGNMENT_STYLES: &[&str] = &["EnforcedColonStyle", "EnforcedHashRocketStyle"];

#[derive(Default)]
pub struct ArgumentAlignment;

impl Cop for ArgumentAlignment {
    fn name(&self) -> &'static str {
        "Layout/ArgumentAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !multiple_arguments(node)
            || (node.is_call() && node.is_method("[]="))
            || autocorrect_incompatible_with_other_cops(cx)
        {
            return;
        }
        let items = flattened_arguments(cx, node);
        let base = base_column(cx, node, items.first().copied());
        check_alignment(cx, &items, Some(base), &message, &|corrector, cx, node, delta| {
            alignment_corrector::correct(
                corrector,
                cx.processed_source,
                cx.config,
                node.map(Target::Node),
                delta,
                false,
            );
        });
    }
}

fn autocorrect_incompatible_with_other_cops(cx: &Investigation<'_, '_>) -> bool {
    with_first_argument_style(cx) && enforce_hash_argument_with_separator(cx)
}

fn flattened_arguments<'a>(cx: &Investigation<'_, '_>, node: Node<'a>) -> Vec<Node<'a>> {
    if fixed_indentation(cx) {
        arguments_with_last_arg_pairs(cx, node)
    } else {
        arguments_or_first_arg_pairs(node)
    }
}

fn arguments_with_last_arg_pairs<'a>(cx: &Investigation<'_, '_>, node: Node<'a>) -> Vec<Node<'a>> {
    let arguments = node.arguments();
    let mut items = arguments[..arguments.len().saturating_sub(1)].to_vec();
    let Some(last_arg) = node.last_argument() else {
        return items;
    };
    if last_arg.is(NodeType::Hash) && !last_arg.has_braces() {
        if enforce_hash_argument_with_separator(cx) {
            return items;
        }
        items.extend(last_arg.pairs());
    } else {
        items.push(last_arg);
    }
    items
}

fn arguments_or_first_arg_pairs(node: Node<'_>) -> Vec<Node<'_>> {
    match node.first_argument() {
        Some(first_arg) if first_arg.is(NodeType::Hash) && !first_arg.has_braces() => first_arg.pairs(),
        _ => node.arguments(),
    }
}

fn multiple_arguments(node: Node<'_>) -> bool {
    if node.arguments().len() >= 2 {
        return true;
    }
    node.first_argument()
        .is_some_and(|first| first.is(NodeType::Hash) && first.pairs().len() >= 2)
}

fn message(cx: &Investigation<'_, '_>, _node: Option<Node<'_>>) -> String {
    if fixed_indentation(cx) {
        FIXED_INDENT_MSG
    } else {
        ALIGN_PARAMS_MSG
    }
    .to_owned()
}

fn fixed_indentation(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyle") == Some("with_fixed_indentation")
}

fn with_first_argument_style(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyle") == Some("with_first_argument")
}

fn base_column(cx: &Investigation<'_, '_>, node: Node<'_>, first_argument: Option<Node<'_>>) -> usize {
    match first_argument {
        Some(first_argument) if !fixed_indentation(cx) => {
            display_column(cx.processed_source, first_argument.source_range())
        }
        _ => {
            let lineno = target_method_lineno(node);
            let line = cx.processed_source.source_line(lineno);
            let indentation_of_line = line
                .chars()
                .position(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
                .unwrap_or(0);
            indentation_of_line + configured_indentation_width(cx)
        }
    }
}

fn target_method_lineno(node: Node<'_>) -> usize {
    match node.loc().selector {
        Some(selector) => node.line_of(selector),
        None => node.line_of(node.loc().begin.unwrap()),
    }
}

/// `config.for_enabled_cop('Layout/HashAlignment')[style]&.include?('separator')`
/// (a string includes a substring; an array includes an element).
fn enforce_hash_argument_with_separator(cx: &Investigation<'_, '_>) -> bool {
    let config = cx.config.for_enabled_cop("Layout/HashAlignment");
    SEPARATOR_ALIGNMENT_STYLES.iter().any(|style| match config.get(*style) {
        Some(Value::String(s)) => s.contains("separator"),
        Some(Value::Sequence(items)) => items.iter().any(|i| i.as_str() == Some("separator")),
        _ => false,
    })
}
