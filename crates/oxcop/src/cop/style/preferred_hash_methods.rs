//! Port of `lib/rubocop/cop/style/preferred_hash_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct PreferredHashMethods;

impl Cop for PreferredHashMethods {
    fn name(&self) -> &'static str {
        "Style/PreferredHashMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["has_key?", "has_value?", "key?", "value?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        let method_name = node.method_name().unwrap_or("");
        let offending: &[&str] = match style {
            "short" => &["has_key?", "has_value?"],
            "verbose" => &["key?", "value?"],
            _ => &[],
        };
        if node.arguments().len() != 1 || !offending.contains(&method_name) {
            return;
        }
        let message = format!(
            "Use `Hash#{}` instead of `Hash#{method_name}`.",
            proper_method_name(style, method_name)
        );
        let selector = node.loc().selector.unwrap();
        let selector_source = node.range_source(selector);
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.replace(selector, proper_method_name(style, selector_source));
        });
    }
}

fn proper_method_name(style: &str, method_name: &str) -> String {
    if style == "verbose" {
        format!("has_{method_name}")
    } else {
        method_name
            .strip_prefix("has_")
            .unwrap_or(method_name)
            .to_owned()
    }
}
