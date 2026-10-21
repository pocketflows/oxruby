//! Port of `lib/rubocop/cop/layout/first_parameter_indentation.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::multiline_element_indentation::{IndentBase, Settings, check_first};
use crate::cop::{Cop, Investigation};

const BRACE_ALIGNMENT_STYLE: &str = "align_parentheses";

#[derive(Default)]
pub struct FirstParameterIndentation;

impl Cop for FirstParameterIndentation {
    fn name(&self) -> &'static str {
        "Layout/FirstParameterIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let arguments = node.arguments();
        if arguments.is_empty() {
            return;
        }
        let Some(left_parenthesis) = node.arguments_node().and_then(|a| a.loc().begin) else {
            return;
        };
        if arguments.len() >= 2
            && style(cx) == BRACE_ALIGNMENT_STYLE
            && enforce_parameter_with_fixed_indentation(cx)
        {
            return;
        }
        let first_elem = arguments[0];
        if first_elem.first_line() == cx.processed_source.line_of(left_parenthesis.begin_pos) {
            return;
        }
        let settings = Settings {
            style: style(cx),
            brace_alignment_style: BRACE_ALIGNMENT_STYLE,
        };
        check_first(
            cx,
            settings,
            first_elem,
            left_parenthesis,
            None,
            0,
            &message,
            &|corrector, cx, node, delta| {
                alignment_corrector::correct(
                    corrector,
                    cx.processed_source,
                    cx.config,
                    Some(Target::Node(node)),
                    delta,
                    false,
                );
            },
        );
    }
}

fn message(cx: &Investigation<'_, '_>, _: IndentBase) -> String {
    let base_description = if style(cx) == BRACE_ALIGNMENT_STYLE {
        "the position of the opening parenthesis"
    } else {
        "the start of the line where the left parenthesis is"
    };
    format!(
        "Use {} spaces for indentation in method args, relative to {base_description}.",
        configured_indentation_width(cx)
    )
}

fn enforce_parameter_with_fixed_indentation(cx: &Investigation<'_, '_>) -> bool {
    cx.config
        .for_enabled_cop("Layout/ParameterAlignment")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        == Some("with_fixed_indentation")
}
