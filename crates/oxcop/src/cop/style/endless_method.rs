//! Port of `lib/rubocop/cop/style/endless_method.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::endless_method_rewriter::{
    arguments, configured_indentation_width, correct_to_multiline, receiver,
};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Avoid endless method definitions.";
const MSG_MULTI_LINE: &str = "Avoid endless method definitions with multiple lines.";
const MSG_REQUIRE_SINGLE: &str = "Use endless method definitions for single line methods.";
const MSG_REQUIRE_ALWAYS: &str = "Use endless method definitions.";

#[derive(Default)]
pub struct EndlessMethod;

impl Cop for EndlessMethod {
    fn name(&self) -> &'static str {
        "Style/EndlessMethod"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_assignment_method() || use_heredoc(node) {
            return;
        }
        let width = configured_indentation_width(cx);
        match style(cx) {
            "allow_single_line" | "allow_always" => {
                if !node.is_endless() || node.is_single_line() || style(cx) == "allow_always" {
                    return;
                }
                cx.add_offense_with_correction(node.source_range(), MSG_MULTI_LINE, |corrector| {
                    if !endless_parent(node) {
                        correct_to_multiline(corrector, node, width);
                    }
                });
            }
            "disallow" => {
                if !node.is_endless() {
                    return;
                }
                cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                    if !endless_parent(node) {
                        correct_to_multiline(corrector, node, width);
                    }
                });
            }
            "require_single_line" => {
                if node.is_endless() && node.is_multiline() {
                    cx.add_offense_with_correction(
                        node.source_range(),
                        MSG_MULTI_LINE,
                        |corrector| correct_to_multiline(corrector, node, width),
                    );
                } else if !node.is_endless()
                    && can_be_made_endless(node)
                    && !endless_replacement(node).contains('\n')
                {
                    if too_long_when_made_endless(cx, node) {
                        return;
                    }
                    cx.add_offense_with_correction(
                        node.source_range(),
                        MSG_REQUIRE_SINGLE,
                        |corrector| correct_to_endless(corrector, node),
                    );
                }
            }
            "require_always" => {
                if node.is_endless() || !can_be_made_endless(node) {
                    return;
                }
                if too_long_when_made_endless(cx, node) {
                    return;
                }
                cx.add_offense_with_correction(node.source_range(), MSG_REQUIRE_ALWAYS, |corrector| {
                    correct_to_endless(corrector, node);
                });
            }
            _ => {}
        }
    }
}

fn endless_parent(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is_any_def() && p.is_endless())
}

fn use_heredoc(node: Node<'_>) -> bool {
    let Some(body) = node.body() else {
        return false;
    };
    if body.is_any_str() && body.is_heredoc() {
        return true;
    }
    body.descendants()
        .any(|d| d.is(NodeType::Str) && d.is_heredoc())
}

fn correct_to_endless(corrector: &mut Corrector, node: Node<'_>) {
    let body = node.body().unwrap();
    corrector.replace(signature_to_body_range(node), " = ");
    corrector.remove(
        body.source_range()
            .end()
            .join(node.loc().end.unwrap().end()),
    );
}

fn signature_to_body_range(node: Node<'_>) -> Range {
    let signature_end = match node.arguments_node() {
        Some(args) if args.child_nodes().next().is_some() => args.source_range().end(),
        _ => node.loc().name.unwrap().end(),
    };
    signature_end.join(node.body().unwrap().source_range().begin())
}

fn endless_replacement(node: Node<'_>) -> String {
    format!(
        "def {}{}{} = {}",
        receiver(node),
        node.method_name().unwrap_or(""),
        arguments(node, ""),
        node.body().map_or("", |b| b.source())
    )
    .trim()
    .to_owned()
}

fn can_be_made_endless(node: Node<'_>) -> bool {
    node.body().is_some_and(|b| {
        !matches!(
            b.ty(),
            NodeType::Begin | NodeType::Kwbegin | NodeType::Rescue | NodeType::Ensure
        )
    })
}

fn too_long_when_made_endless(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    const COP: &str = "Layout/LineLength";
    if !cx.config.cop_enabled(COP) {
        return false;
    }
    let max = cx
        .config
        .for_cop(COP)
        .get("Max")
        .and_then(Value::as_i64)
        .unwrap_or(120);
    (endless_replacement(node).chars().count() + node.column()) as i64 > max
}
