//! Port of `lib/rubocop/cop/lint/implicit_string_concatenation.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};

const FOR_ARRAY: &str =
    " Or, if they were intended to be separate array elements, separate them with a comma.";
const FOR_METHOD: &str =
    " Or, if they were intended to be separate method arguments, separate them with a comma.";

#[derive(Default)]
pub struct ImplicitStringConcatenation;

impl Cop for ImplicitStringConcatenation {
    fn name(&self) -> &'static str {
        "Lint/ImplicitStringConcatenation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for (lhs_node, rhs_node) in each_bad_cons(node) {
            let range = lhs_node.source_range().join(rhs_node.source_range());
            let mut message = format!(
                "Combine {} and {} into a single string literal, rather than using implicit string concatenation.",
                display_str(lhs_node),
                display_str(rhs_node)
            );
            if node.parent().is_some_and(|p| p.is(NodeType::Array)) {
                message.push_str(FOR_ARRAY);
            } else if node.parent().is_some_and(|p| p.is(NodeType::Send)) {
                message.push_str(FOR_METHOD);
            }
            cx.add_offense_with_correction(range, &message, |corrector| {
                if string_value(lhs_node).is_empty() {
                    corrector.remove(lhs_node.source_range());
                } else if string_value(rhs_node).is_empty() {
                    corrector.remove(rhs_node.source_range());
                } else {
                    let range = lhs_node
                        .source_range()
                        .end()
                        .join(rhs_node.source_range().begin());
                    corrector.replace(range, " + ");
                }
            });
        }
    }
}

fn each_bad_cons<'a>(node: Node<'a>) -> Vec<(Node<'a>, Node<'a>)> {
    let children: Vec<Node<'a>> = node.child_nodes().collect();
    // `children.each_cons(2)` sees non-node children too, but a `dstr` only
    // has node children.
    children
        .windows(2)
        .filter_map(|pair| {
            let (child_node1, child_node2) = (pair[0], pair[1]);
            if !string_literals(child_node1, child_node2) {
                return None;
            }
            if child_node1.last_line() != child_node2.first_line() {
                return None;
            }
            let last = child_node1.source().chars().last();
            if last.is_none() || last != ending_delimiter(child_node1) {
                return None;
            }
            Some((child_node1, child_node2))
        })
        .collect()
}

fn ending_delimiter(str: Node<'_>) -> Option<char> {
    match str.source().chars().next() {
        Some('\'') => Some('\''),
        Some('"') => Some('"'),
        _ => None,
    }
}

fn string_literal(node: Node<'_>) -> bool {
    node.is_any(&[NodeType::Str, NodeType::Dstr])
}

fn string_literals(node1: Node<'_>, node2: Node<'_>) -> bool {
    string_literal(node1) && string_literal(node2)
}

fn display_str(node: Node<'_>) -> String {
    if node.source().contains('\n') {
        ruby_inspect(&str_content(node))
    } else {
        node.source().to_owned()
    }
}

fn str_content(node: Node<'_>) -> String {
    if node.is(NodeType::Str) {
        return node.str_content().unwrap_or_default().to_owned();
    }
    node.children()
        .iter()
        .map(|child| match child {
            Child::Node(id) => str_content(node.ast().node(*id)),
            _ => String::new(),
        })
        .collect()
}

/// `StrNode#value` / `DstrNode#value`.
fn string_value(node: Node<'_>) -> String {
    if node.is(NodeType::Str) {
        return node.str_content().unwrap_or_default().to_owned();
    }
    node.child_nodes()
        .map(|child| {
            if child.is_any(&[NodeType::Str, NodeType::Dstr]) {
                string_value(child)
            } else {
                child.source().to_owned()
            }
        })
        .collect()
}
