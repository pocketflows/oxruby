//! Port of `lib/rubocop/cop/style/empty_method.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::parentheses;
use crate::cop::{Cop, Investigation};

const MSG_COMPACT: &str = "Put empty method definitions on a single line.";
const MSG_EXPANDED: &str = "Put the `end` of empty method definitions on the next line.";

#[derive(Default)]
pub struct EmptyMethod;

impl Cop for EmptyMethod {
    fn name(&self) -> &'static str {
        "Style/EmptyMethod"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.body().is_some() || cx.processed_source.contains_comment(node.source_range()) {
            return;
        }
        let compact_style = style(cx) == "compact";
        let expanded_style = style(cx) == "expanded";
        let correct_style = (compact_style && node.is_single_line())
            || (expanded_style && node.is_multiline());
        if correct_style || compact_style_disallowed(cx, compact_style) {
            return;
        }
        let message = if compact_style {
            MSG_COMPACT
        } else {
            MSG_EXPANDED
        };
        let correction = corrected(node, compact_style);
        let exceeds = compact_style
            && max_line_length(cx).is_some_and(|max| correction.chars().count() as i64 > max);
        cx.add_offense_with_correction(node.source_range(), message, |corrector| {
            if exceeds {
                return;
            }
            corrector.replace(node.source_range(), correction);
        });
    }
}

fn compact_style_disallowed(cx: &Investigation<'_, '_>, compact_style: bool) -> bool {
    if !compact_style {
        return false;
    }
    const COP: &str = "Style/SingleLineMethods";
    cx.config.cop_enabled(COP)
        && cx.config.for_cop(COP).get("AllowIfMethodIsEmpty") == Some(&Value::Bool(false))
}

/// `max_line_length` (`AutocorrectLogic`).
fn max_line_length(cx: &Investigation<'_, '_>) -> Option<i64> {
    const COP: &str = "Layout/LineLength";
    if !cx.config.cop_enabled(COP) {
        return None;
    }
    Some(
        cx.config
            .for_cop(COP)
            .get("Max")
            .and_then(Value::as_i64)
            .unwrap_or(120),
    )
}

fn corrected(node: Node<'_>, compact_style: bool) -> String {
    let scope = node
        .receiver()
        .map_or(String::new(), |r| format!("{}.", r.source()));
    let arguments = if node.has_arguments() {
        let args: Vec<&str> = node.arguments().iter().map(|a| a.source()).collect();
        let args = args.join(", ");
        if node.arguments_node().is_some_and(parentheses) {
            format!("({args})")
        } else {
            format!(" {args}")
        }
    } else {
        String::new()
    };
    let signature = format!("{scope}{}{arguments}", node.method_name().unwrap_or(""));
    let joint = if compact_style {
        "; ".to_owned()
    } else {
        format!("\n{}", " ".repeat(node.column()))
    };
    format!("def {signature}{joint}end")
}
