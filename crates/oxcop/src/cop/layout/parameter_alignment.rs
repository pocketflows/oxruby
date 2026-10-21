//! Port of `lib/rubocop/cop/layout/parameter_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{check_alignment, configured_indentation_width, display_column};
use crate::cop::{Cop, Investigation};

const ALIGN_PARAMS_MSG: &str =
    "Align the parameters of a method definition if they span more than one line.";
const FIXED_INDENT_MSG: &str = "Use one level of indentation for parameters following the first line of a multi-line method definition.";

#[derive(Default)]
pub struct ParameterAlignment;

impl Cop for ParameterAlignment {
    fn name(&self) -> &'static str {
        "Layout/ParameterAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let args = node.arguments();
        if args.len() < 2 {
            return;
        }
        let base = base_column(cx, node, &args);
        check_alignment(cx, &args, Some(base), &message, &|corrector, cx, node, delta| {
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

fn base_column(cx: &Investigation<'_, '_>, node: Node<'_>, args: &[Node<'_>]) -> usize {
    if fixed_indentation(cx) {
        let lineno = target_method_lineno(node);
        let line = cx.processed_source.source_line(lineno);
        let indentation_of_line = line
            .chars()
            .position(|c| !matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c'))
            .unwrap_or(0);
        indentation_of_line + configured_indentation_width(cx)
    } else {
        display_column(cx.processed_source, args[0].source_range())
    }
}

fn target_method_lineno(node: Node<'_>) -> usize {
    node.line_of(node.loc().keyword.unwrap())
}
