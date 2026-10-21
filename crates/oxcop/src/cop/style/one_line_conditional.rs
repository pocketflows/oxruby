//! Port of `lib/rubocop/cop/style/one_line_conditional.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::if_then_corrector::IfThenCorrector;
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::on_normal_if_unless::normal_if_unless;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct OneLineConditional {
    ignored: IgnoredNodes,
}

impl Cop for OneLineConditional {
    fn name(&self) -> &'static str {
        "Style/OneLineConditional"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !normal_if_unless(node) || !node.is_single_line() || node.else_branch().is_none() {
            return;
        }
        if node.is_elsif() || node.if_branch().is_some_and(|b| b.is(NodeType::Begin)) {
            return;
        }
        let multiline =
            cx.cop_config_bool("AlwaysCorrectToMultiline", false) || cannot_replace_to_ternary(node);
        let keyword = node.keyword();
        let message = if multiline {
            format!("Favor multi-line `{keyword}` over single-line `{keyword}/then/else/end` constructs.")
        } else {
            format!(
                "Favor the ternary operator (`?:`) over single-line `{keyword}/then/else/end` constructs."
            )
        };
        let indentation = configured_indentation_width(cx);
        let ignored = &mut self.ignored;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if ignored.part_of(node) {
                return;
            }
            if multiline {
                IfThenCorrector::new(node, Some(indentation)).call(corrector);
            } else {
                corrector.replace(node.source_range(), ternary_correction(node));
            }
            ignored.ignore(node);
        });
    }
}

fn cannot_replace_to_ternary(node: Node<'_>) -> bool {
    if node.is_elsif_conditional() {
        return true;
    }
    let else_branch = node.else_branch().unwrap();
    else_branch.is(NodeType::Begin) && else_branch.child_nodes().count() >= 2
}

fn ternary_correction(node: Node<'_>) -> String {
    let replaced = format!(
        "{} ? {} : {}",
        expr_replacement(node.child(0)),
        expr_replacement(node.child(1)),
        expr_replacement(node.child(2))
    );
    match node.parent() {
        Some(parent)
            if parent.is_operator_keyword()
                || (parent.is_send() && parent.is_operator_method()) =>
        {
            format!("({replaced})")
        }
        _ => replaced,
    }
}

fn expr_replacement(node: Option<Node<'_>>) -> String {
    let Some(node) = node else {
        return "nil".to_owned();
    };
    if requires_parentheses(node) {
        format!("({})", node.source())
    } else {
        node.source().to_owned()
    }
}

fn requires_parentheses(node: Node<'_>) -> bool {
    if matches!(node.ty(), NodeType::And | NodeType::Or | NodeType::If) || node.is_assignment() {
        return true;
    }
    if node.is_send()
        && node.has_arguments()
        && !node.is_parenthesized_call()
        && !node.is_operator_method()
    {
        return true;
    }
    keyword_with_changed_precedence(node)
}

fn keyword_with_changed_precedence(node: Node<'_>) -> bool {
    if !node.is_keyword() {
        return false;
    }
    if node.is_send() && node.is_prefix_not() {
        return true;
    }
    node.has_arguments() && !node.is_parenthesized_call()
}
