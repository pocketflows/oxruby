//! Port of `lib/rubocop/cop/style/lambda_call.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct LambdaCall {
    ignored: IgnoredNodes,
}

impl Cop for LambdaCall {
    fn name(&self) -> &'static str {
        "Style/LambdaCall"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["call"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_none() {
            return;
        }
        // Rewriting the call rebuilds it as a single expression, which would
        // drop any comments inside the argument list, so leave it to be fixed
        // manually.
        if comments_in_node(cx, node) {
            return;
        }
        let style = style(cx);
        if !offense(style, node) {
            return;
        }
        let prefer = prefer(style, node);
        let message = format!(
            "Prefer the use of `{prefer}` over `{}`.",
            node.source()
        );
        let ignored = &mut self.ignored;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if ignored.part_of(node) {
                return;
            }
            corrector.replace(node.source_range(), &prefer);
            ignored.ignore(node);
        });
    }
}

fn comments_in_node(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let range = node.source_range();
    cx.processed_source.comments().iter().any(|comment| {
        range.begin_pos <= comment.range.begin_pos && comment.range.end_pos <= range.end_pos
    })
}

fn offense(style: &str, node: Node<'_>) -> bool {
    (style == "call" && node.is_implicit_call()) || (style == "braces" && !node.is_implicit_call())
}

fn prefer(style: &str, node: Node<'_>) -> String {
    let receiver = node.receiver().unwrap().source();
    let dot = node.range_source(node.loc().dot.unwrap());
    let arguments: Vec<&str> = node.arguments().iter().map(|a| a.source()).collect();
    let arguments = arguments.join(", ");
    let call_arguments = if node.arguments().is_empty() {
        String::new()
    } else {
        format!("({arguments})")
    };
    let method = if style == "call" {
        format!("call{call_arguments}")
    } else {
        format!("({arguments})")
    };
    format!("{receiver}{dot}{method}")
}
