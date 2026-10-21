//! Port of `lib/rubocop/cop/style/array_first_last.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct ArrayFirstLast;

impl Cop for ArrayFirstLast {
    fn name(&self) -> &'static str {
        "Style/ArrayFirstLast"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let arguments = node.arguments();
        if arguments.len() != 1 {
            return;
        }
        let Some(value) = arguments[0].int_value() else {
            return;
        };
        if value != 0 && value != -1 {
            return;
        }
        let node = innermost_braces_node(node);
        if node.parent().is_some_and(brace_method) {
            return;
        }
        if compound_assignment_target(node) {
            return;
        }
        let preferred = if value == 0 { "first" } else { "last" };
        let offense_range = find_offense_range(node);
        let message = format!("Use `{preferred}`.");
        let replacement = if node.loc().dot.is_some() {
            preferred.to_owned()
        } else {
            format!(".{preferred}")
        };
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            corrector.replace(offense_range, replacement);
        });
    }
}

fn find_offense_range(node: Node<'_>) -> Range {
    let selector = node.loc().selector.unwrap();
    if node.loc().dot.is_some() {
        selector.join(node.source_range().end())
    } else {
        selector
    }
}

fn innermost_braces_node(mut node: Node<'_>) -> Node<'_> {
    while let Some(receiver) = node.receiver().filter(|r| r.is_send() && r.is_method("[]")) {
        node = receiver;
    }
    node
}

fn brace_method(node: Node<'_>) -> bool {
    node.is_send() && (node.is_method("[]") || node.is_method("[]="))
}

fn compound_assignment_target(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        matches!(
            p.ty(),
            NodeType::OpAsgn | NodeType::OrAsgn | NodeType::AndAsgn
        ) && p.child(0).is_some_and(|c| c.same_as(node))
    })
}
