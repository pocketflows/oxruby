//! Port of `lib/rubocop/cop/style/option_hash.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer keyword arguments to options hashes.";

#[derive(Default)]
pub struct OptionHash;

impl Cop for OptionHash {
    fn name(&self) -> &'static str {
        "Style/OptionHash"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Args]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if super_used(node) {
            return;
        }
        let method_name = node.parent().and_then(|p| p.method_name()).unwrap_or("");
        if cx.cop_config_list("Allowlist").contains(&method_name) {
            return;
        }
        // (args ... $(optarg [#suspicious_name? _] (hash)))
        let Some(options) = node.child_nodes().last() else {
            return;
        };
        if !options.is(NodeType::Optarg) {
            return;
        }
        let name = options.name().unwrap_or("");
        let value = options.child(1);
        if suspicious_name(cx, name)
            && value.is_some_and(|v| v.is(NodeType::Hash) && v.children().is_empty())
        {
            cx.add_offense(options.source_range(), MSG);
        }
    }
}

fn suspicious_name(cx: &Investigation<'_, '_>, arg_name: &str) -> bool {
    cx.cop_config_value("SuspiciousParamNames").is_some()
        && cx.cop_config_list("SuspiciousParamNames").contains(&arg_name)
}

fn super_used(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        p.each_node()
            .any(|n| matches!(n.ty(), NodeType::Zsuper | NodeType::Super))
    })
}
