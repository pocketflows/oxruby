//! Port of rubocop-rails' `lib/rubocop/cop/rails/http_status_name_consistency.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const PREFERRED_STATUSES: &[(&str, &str)] = &[
    ("unprocessable_entity", "unprocessable_content"),
    ("payload_too_large", "content_too_large"),
];

static STATUS_METHOD_CALL: NodePattern = NodePattern::new(
    "{
       (send nil? {:render :redirect_to} _ $hash)
       (send nil? {:render :redirect_to} $hash)
       (send nil? {:head :assert_response} $_ ...)
       (send nil? :assert_redirected_to _ $hash ...)
       (send nil? :assert_redirected_to $hash ...)
     }",
);

static STATUS_HASH_VALUE: NodePattern = NodePattern::new("(hash <(pair (sym :status) $_) ...>)");

#[derive(Default)]
pub struct HttpStatusNameConsistency;

impl Cop for HttpStatusNameConsistency {
    fn name(&self) -> &'static str {
        "Rails/HttpStatusNameConsistency"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn gem_requirements(&self) -> &'static [(&'static str, &'static [&'static str])] {
        &[("rack", &[">= 3.1.0"])]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "render",
            "redirect_to",
            "head",
            "assert_response",
            "assert_redirected_to",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(status_node) = STATUS_METHOD_CALL
            .match_node(node)
            .and_then(|captures| captures.first().and_then(|c| c.node()))
        else {
            return;
        };
        if status_node.is(NodeType::Hash) {
            // Handle hash arguments like { status: :unprocessable_entity }
            if let Some(status_value) = STATUS_HASH_VALUE
                .match_node(status_node)
                .and_then(|captures| captures.first().and_then(|c| c.node()))
            {
                check_status_name_consistency(cx, status_value);
            }
        } else {
            // Handle positional arguments like head :unprocessable_entity
            check_status_name_consistency(cx, status_node);
        }
    }
}

fn check_status_name_consistency(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let preferred = node.sym_value().and_then(|current| {
        PREFERRED_STATUSES
            .iter()
            .find(|(name, _)| *name == current)
    });
    if let Some((current_status, preferred_status)) = preferred {
        let message = format!("Prefer `:{preferred_status}` over `:{current_status}`.");
        let range = node.source_range();
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, format!(":{preferred_status}"));
        });
    } else {
        for child in node.child_nodes() {
            check_status_name_consistency(cx, child);
        }
    }
}
