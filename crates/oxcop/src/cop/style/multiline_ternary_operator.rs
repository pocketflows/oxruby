//! Port of `lib/rubocop/cop/style/multiline_ternary_operator.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::comments_in_range;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

const MSG_IF: &str = "Avoid multi-line ternary operators, use `if` or `unless` instead.";
const MSG_SINGLE_LINE: &str = "Avoid multi-line ternary operators, use single-line instead.";
const SINGLE_LINE_TYPES: &[NodeType] = &[
    NodeType::Return,
    NodeType::Break,
    NodeType::Next,
    NodeType::Send,
    NodeType::Csend,
];

#[derive(Default)]
pub struct MultilineTernaryOperator {
    ignored: IgnoredNodes,
}

impl Cop for MultilineTernaryOperator {
    fn name(&self) -> &'static str {
        "Style/MultilineTernaryOperator"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_ternary() && node.is_multiline() && node.source() != replacement(node)) {
            return;
        }
        let message = if enforce_single_line_ternary_operator(node) {
            MSG_SINGLE_LINE
        } else {
            MSG_IF
        };
        let ignored = &mut self.ignored;
        cx.add_offense_cx(node.source_range(), message, |corrector, cx| {
            if ignored.part_of(node) {
                return;
            }
            corrector.replace(node.source_range(), replacement(node));
            if let Some(parent) = node.parent() {
                let comments: String = comments_in_range(cx.processed_source, node)
                    .iter()
                    .map(|c| format!("{}\n", c.text))
                    .collect();
                if !comments.is_empty() {
                    corrector.insert_before(parent.source_range(), comments);
                }
            }
            ignored.ignore(node);
        });
    }
}

fn replacement(node: Node<'_>) -> String {
    let condition = node.condition().unwrap().source();
    let if_branch = node.if_branch().map_or("", |b| b.source());
    let else_branch = node.else_branch().map_or("", |b| b.source());
    if enforce_single_line_ternary_operator(node) {
        format!("{condition} ? {if_branch} : {else_branch}")
    } else {
        format!("if {condition}\n  {if_branch}\nelse\n  {else_branch}\nend")
    }
}

fn enforce_single_line_ternary_operator(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| {
        p.is_any(SINGLE_LINE_TYPES) && !(p.is(NodeType::Send) && p.is_assignment_method())
    })
}
