//! Port of `lib/rubocop/cop/style/method_def_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::{add_parentheses, parentheses};
use crate::cop::{Cop, Investigation};

const MSG_PRESENT: &str = "Use def without parentheses.";
const MSG_MISSING: &str = "Use def with parentheses when there are parameters.";

#[derive(Default)]
pub struct MethodDefParentheses;

impl Cop for MethodDefParentheses {
    fn name(&self) -> &'static str {
        "Style/MethodDefParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/ArgumentsForwarding"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(args) = node.arguments_node() else {
            return;
        };
        let style = style(cx);
        let src = cx.processed_source.buffer_source();
        if require_parentheses(style, args) {
            if node.has_arguments() && !parentheses(args) {
                cx.add_offense_with_correction(args.source_range(), MSG_MISSING, |corrector| {
                    add_parentheses(src, args, corrector);
                });
            }
        } else if forced_parentheses(node) {
        } else if parentheses(args) {
            cx.add_offense_with_correction(args.source_range(), MSG_PRESENT, |corrector| {
                corrector.replace(args.loc().begin.unwrap(), " ");
                corrector.remove(args.loc().end.unwrap());
            });
        }
    }
}

/// Regardless of style, parentheses are necessary for endless methods and
/// argument lists with anonymous forwarding (`...`, `*`, `**`, `&`).
fn forced_parentheses(node: Node<'_>) -> bool {
    node.is_endless() || anonymous_arguments(node)
}

fn require_parentheses(style: &str, args: Node<'_>) -> bool {
    style == "require_parentheses"
        || (style == "require_no_parentheses_except_multiline" && args.is_multiline())
}

fn anonymous_arguments(node: Node<'_>) -> bool {
    if node.arguments().iter().any(|arg| {
        arg.is(NodeType::ForwardArg)
            || (arg.is_any(&[NodeType::Restarg, NodeType::Kwrestarg]) && arg.name().is_none())
    }) {
        return true;
    }
    node.last_argument()
        .is_some_and(|last| last.is(NodeType::Blockarg) && last.name().is_none())
}
