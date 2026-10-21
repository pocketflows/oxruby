//! Port of `lib/rubocop/cop/style/single_line_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::line_break_corrector::{break_line_before, move_comment};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Avoid single-line method definitions.";

#[derive(Default)]
pub struct SingleLineMethods;

impl Cop for SingleLineMethods {
    fn name(&self) -> &'static str {
        "Style/SingleLineMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_single_line() || node.is_endless() {
            return;
        }
        if cx.cop_config_bool("AllowIfMethodIsEmpty", false) && node.body().is_none() {
            return;
        }
        let endless = correct_to_endless(cx, node.body());
        let width = configured_indentation_width(cx);
        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            if endless {
                correct_to_endless_def(corrector, node);
            } else {
                correct_to_multiline(corrector, node, width);
                move_comment(ps.comment_at_line(node.first_line()), node, corrector);
            }
        });
    }
}

/// `Alignment#configured_indentation_width`.
fn configured_indentation_width(cx: &Investigation<'_, '_>) -> usize {
    cx.cop_config_int("IndentationWidth")
        .or_else(|| {
            cx.config
                .for_cop("Layout/IndentationWidth")
                .get("Width")
                .and_then(serde_yaml_ng::Value::as_i64)
        })
        .unwrap_or(2) as usize
}

/// `correct_to_endless?(body_node)`.
fn correct_to_endless(cx: &Investigation<'_, '_>, body_node: Option<Node<'_>>) -> bool {
    if cx.target_ruby_version() < 3.0 || disallow_endless_method_style(cx) {
        return false;
    }
    let Some(body_node) = body_node else {
        return false;
    };
    if body_node.is_basic_conditional()
        || body_node.parent().is_some_and(|p| p.is_assignment_method())
        || body_node.is_any(&[NodeType::Return, NodeType::Break, NodeType::Next])
    {
        return false;
    }
    !body_node.is_any(&[NodeType::Begin, NodeType::Kwbegin])
}

fn correct_to_multiline(corrector: &mut Corrector, node: Node<'_>, width: usize) {
    match node.body() {
        Some(body) if body.is(NodeType::Begin) && body.is_parenthesized_call() => {
            break_line_before(body.source_range(), node, corrector, width, 1);
        }
        body => {
            for part in each_part(body) {
                break_line_before(part, node, corrector, width, 1);
            }
        }
    }
    break_line_before(node.loc().end.unwrap(), node, corrector, width, 0);
}

fn correct_to_endless_def(corrector: &mut Corrector, node: Node<'_>) {
    let receiver = node
        .receiver()
        .map(|r| format!("{}.", r.source()))
        .unwrap_or_default();
    let arguments = if node.has_arguments() {
        node.arguments_node().unwrap().source().to_owned()
    } else {
        "()".to_owned()
    };
    let body_source = method_body_source(node.body().unwrap());
    let replacement = format!(
        "def {receiver}{}{arguments} = {body_source}",
        node.method_name().unwrap()
    );
    corrector.replace(node.source_range(), replacement);
}

fn each_part(body: Option<Node<'_>>) -> Vec<Range> {
    match body {
        None => Vec::new(),
        Some(body) if body.is(NodeType::Begin) => {
            body.child_nodes().map(|part| part.source_range()).collect()
        }
        Some(body) => vec![body.source_range()],
    }
}

fn method_body_source(method_body: Node<'_>) -> String {
    if require_parentheses(method_body) {
        let arguments_source = method_body
            .arguments()
            .iter()
            .map(|a| a.source())
            .collect::<Vec<_>>()
            .join(", ");
        let body_source = format!(
            "{}({arguments_source})",
            method_body.method_name().unwrap()
        );
        match method_body.receiver() {
            Some(receiver) => format!("{}.{body_source}", receiver.source()),
            None => body_source,
        }
    } else {
        method_body.source().to_owned()
    }
}

fn require_parentheses(method_body: Node<'_>) -> bool {
    if !method_body.is_send() || method_body.is_arithmetic_operation() {
        return false;
    }
    method_body.has_arguments() && !method_body.is_comparison_method()
}

fn disallow_endless_method_style(cx: &Investigation<'_, '_>) -> bool {
    if !cx.config.cop_enabled("Style/EndlessMethod") {
        return true;
    }
    cx.config
        .for_cop("Style/EndlessMethod")
        .get("EnforcedStyle")
        .and_then(serde_yaml_ng::Value::as_str)
        == Some("disallow")
}
