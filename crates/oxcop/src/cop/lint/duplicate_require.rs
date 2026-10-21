//! Port of `lib/rubocop/cop/lint/duplicate_require.rb`.

use std::collections::{HashMap, HashSet};

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeId, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};

static REQUIRE_CALL: NodePattern =
    NodePattern::new("(send {nil? (const _ :Kernel)} {:require :require_relative} _)");

#[derive(Default)]
pub struct DuplicateRequire {
    /// `@required`, keyed by the identity of the call's parent.
    required: HashMap<Option<NodeId>, HashSet<String>>,
}

impl Cop for DuplicateRequire {
    fn name(&self) -> &'static str {
        "Lint/DuplicateRequire"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["require", "require_relative"]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.required.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !REQUIRE_CALL.is_match(node) {
            return;
        }
        let method_name = node.method_name().unwrap();
        let key = format!("{method_name}{}", sexp(node.first_argument().unwrap()));
        if self
            .required
            .entry(node.parent().map(|p| p.id()))
            .or_default()
            .insert(key)
        {
            return;
        }
        let message = format!("Duplicate `{method_name}` detected.");
        cx.add_offense_cx(node.source_range(), &message, |corrector, cx| {
            corrector.remove(range_by_whole_lines(
                cx.processed_source,
                node.source_range(),
                true,
            ));
        });
    }
}

/// The node's s-expression (`Node#to_s`), as a structural key.
fn sexp(node: Node<'_>) -> String {
    let mut out = format!("({}", node.ty().name());
    for child in node.children() {
        out.push(' ');
        match child {
            Child::Node(id) => out.push_str(&sexp(node.ast().node(*id))),
            other => out.push_str(&format!("{other:?}")),
        }
    }
    out.push(')');
    out
}
