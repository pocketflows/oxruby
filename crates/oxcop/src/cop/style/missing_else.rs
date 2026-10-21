//! Port of `lib/rubocop/cop/style/missing_else.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::on_normal_if_unless::normal_if_unless;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct MissingElse;

impl Cop for MissingElse {
    fn name(&self) -> &'static str {
        "Style/MissingElse"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::Case]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        if node.is(NodeType::If) {
            if !normal_if_unless(node) || style == "case" {
                return;
            }
            if unless_else_cop_enabled(cx) && node.is_unless() {
                return;
            }
        } else if style == "if" {
            return;
        }
        check(cx, node);
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if node.loc().else_.is_some() {
        return;
    }
    let empty_else_style = empty_else_style(cx);
    let template = match empty_else_style.as_deref() {
        Some("empty") => "`%<type>s` condition requires an `else`-clause with `nil` in it.",
        Some("nil") => "`%<type>s` condition requires an empty `else`-clause.",
        _ => "`%<type>s` condition requires an `else`-clause.",
    };
    let message = template.replace("%<type>s", node.ty().name());
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        let node = if node.loc().end.is_some() {
            Some(node)
        } else {
            node.ancestors().find(|a| a.loc().end.is_some())
        };
        let Some(end) = node.and_then(|n| n.loc().end) else {
            return;
        };
        match empty_else_style.as_deref() {
            Some("empty") => corrector.insert_before(end, "else; nil; "),
            Some("nil") => corrector.insert_before(end, "else; "),
            _ => {}
        }
    });
}

fn unless_else_cop_enabled(cx: &Investigation<'_, '_>) -> bool {
    !matches!(
        cx.config.for_cop("Style/UnlessElse").get("Enabled"),
        None | Some(Value::Bool(false)) | Some(Value::Null)
    )
}

fn empty_else_style(cx: &Investigation<'_, '_>) -> Option<String> {
    cx.config
        .for_cop("Style/EmptyElse")
        .get("EnforcedStyle")
        .and_then(Value::as_str)
        .map(str::to_owned)
}
