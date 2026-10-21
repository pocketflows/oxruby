//! Port of `lib/rubocop/cop/metrics/method_length.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::code_length::check_code_length;
use crate::cop::{Cop, Investigation};

const LABEL: &str = "Method";

#[derive(Default)]
pub struct MethodLength;

impl Cop for MethodLength {
    fn name(&self) -> &'static str {
        "Metrics/MethodLength"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Def,
            NodeType::Defs,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_def() {
            if allowed(cx, node.method_name().unwrap_or("")) {
                return;
            }
        } else {
            if !node.is_method("define_method") {
                return;
            }
            if let Some(method_name) = node.send_node().and_then(|s| s.first_argument())
                && method_name.is_basic_literal()
                && allowed(cx, &literal_value(method_name))
            {
                return;
            }
        }
        check_code_length(cx, node, LABEL);
    }
}

/// `value.to_s` of a basic literal.
fn literal_value(node: Node<'_>) -> String {
    match node.children().first() {
        Some(Child::Sym(s) | Child::Str(s)) => s.clone(),
        Some(Child::Int(i)) => i.to_string(),
        _ => String::new(),
    }
}

fn allowed(cx: &Investigation<'_, '_>, method_name: &str) -> bool {
    allowed_method(cx, method_name) || matches_allowed_pattern(cx, method_name)
}
