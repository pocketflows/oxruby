//! Port of `lib/rubocop/cop/lint/rescue_type.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const INVALID_TYPES: &[NodeType] = &[
    NodeType::Array,
    NodeType::Complex,
    NodeType::Dstr,
    NodeType::False,
    NodeType::Float,
    NodeType::Hash,
    NodeType::Nil,
    NodeType::Int,
    NodeType::Rational,
    NodeType::Str,
    NodeType::Sym,
    NodeType::True,
];

#[derive(Default)]
pub struct RescueType;

impl Cop for RescueType {
    fn name(&self) -> &'static str {
        "Lint/RescueType"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let invalid = invalid_exceptions(&node.exceptions());
        if invalid.is_empty() {
            return;
        }
        let (Some(keyword), Some(rescued)) = (node.loc().keyword, node.child(0)) else {
            return;
        };

        let message = format!(
            "Rescuing from `{}` will raise a `TypeError` instead of catching the actual exception.",
            invalid
                .iter()
                .map(|e| e.source())
                .collect::<Vec<_>>()
                .join(", ")
        );
        cx.add_offense_with_correction(
            keyword.join(rescued.source_range()),
            &message,
            |corrector| {
                let range = keyword.end().join(rescued.source_range().end());
                let exceptions: Vec<Node<'_>> = rescued.child_nodes().collect();
                corrector.replace(range, correction(&exceptions));
            },
        );
    }
}

fn correction(exceptions: &[Node<'_>]) -> String {
    let correction = valid_exceptions(exceptions)
        .iter()
        .map(|e| e.source())
        .collect::<Vec<_>>()
        .join(", ");
    if correction.is_empty() {
        correction
    } else {
        format!(" {correction}")
    }
}

fn valid_exceptions<'a>(exceptions: &[Node<'a>]) -> Vec<Node<'a>> {
    let invalid = invalid_exceptions(exceptions);
    exceptions
        .iter()
        .copied()
        .filter(|e| !invalid.iter().any(|i| i.same_as(*e)))
        .collect()
}

fn invalid_exceptions<'a>(exceptions: &[Node<'a>]) -> Vec<Node<'a>> {
    exceptions
        .iter()
        .copied()
        .filter(|e| INVALID_TYPES.contains(&e.ty()))
        .collect()
}
