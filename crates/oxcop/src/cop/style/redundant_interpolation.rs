//! Port of `lib/rubocop/cop/style/redundant_interpolation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::percent_literal::percent_literal;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer `to_s` over string interpolation.";

#[derive(Default)]
pub struct RedundantInterpolation;

impl Cop for RedundantInterpolation {
    fn name(&self) -> &'static str {
        "Style/RedundantInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/LineEndConcatenation"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !single_interpolation(node) {
            return;
        }
        let embedded_node = node.child(0).unwrap();
        if use_match_pattern(cx, embedded_node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            if variable_interpolation(embedded_node) {
                corrector.replace(node.source_range(), format!("{}.to_s", embedded_node.source()));
            } else if single_variable_interpolation(embedded_node) {
                let embedded_var = embedded_node.child(0).unwrap();
                let source = if require_parentheses(embedded_var) {
                    let receiver = range_between(
                        embedded_var.source_range().begin_pos,
                        embedded_var.loc().selector.unwrap().end_pos,
                    );
                    let arguments: Vec<&str> =
                        embedded_var.arguments().iter().map(|a| a.source()).collect();
                    format!(
                        "{}({})",
                        embedded_var.range_source(receiver),
                        arguments.join(", ")
                    )
                } else {
                    embedded_var.source().to_owned()
                };
                corrector.replace(node.source_range(), format!("{source}.to_s"));
            } else {
                corrector.replace(node.loc().begin.unwrap(), "");
                corrector.replace(node.loc().end.unwrap(), "");
                corrector.replace(embedded_node.loc().begin.unwrap(), "(");
                corrector.replace(embedded_node.loc().end.unwrap(), ").to_s");
            }
        });
    }
}

fn single_interpolation(node: Node<'_>) -> bool {
    node.children().len() == 1
        && node.child(0).is_some_and(interpolation)
        && !node.parent().is_some_and(|p| p.is(NodeType::Dstr))
        && !node
            .parent()
            .is_some_and(|p| p.is(NodeType::Array) && percent_literal(p))
}

fn use_match_pattern(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if cx.target_ruby_version() <= 2.7 {
        return false;
    }
    node.child_nodes().any(|c| c.is(NodeType::MatchPattern))
}

fn single_variable_interpolation(node: Node<'_>) -> bool {
    if node.children().len() != 1 {
        return false;
    }
    let Some(first_child) = node.child(0) else {
        return false;
    };
    variable_interpolation(first_child) || (first_child.is_send() && !first_child.is_operator_method())
}

fn interpolation(node: Node<'_>) -> bool {
    variable_interpolation(node) || node.is(NodeType::Begin)
}

fn variable_interpolation(node: Node<'_>) -> bool {
    node.is_variable() || node.is_reference()
}

fn require_parentheses(node: Node<'_>) -> bool {
    node.is_send() && node.has_arguments() && !node.is_parenthesized_call()
}
