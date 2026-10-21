//! Port of `lib/rubocop/cop/naming/block_forwarding.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::add_parentheses;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct BlockForwarding;

impl Cop for BlockForwarding {
    fn name(&self) -> &'static str {
        "Naming/BlockForwarding"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &[
            "Lint/AmbiguousOperator",
            "Style/ArgumentsForwarding",
            "Style/ExplicitBlockArgument",
            "Style/MethodDefParentheses",
        ]
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.1)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.arguments().is_empty() {
            return;
        }
        let last_argument = node.last_argument().unwrap();
        if expected_block_forwarding_style(cx, node, last_argument) {
            return;
        }
        let mut forwarded_args = Vec::new();
        for block_pass in node.descendants().filter(|d| d.is(NodeType::BlockPass)) {
            if invalidates_syntax(cx, block_pass) {
                return;
            }
            if block_argument_name_matched(block_pass, last_argument) {
                forwarded_args.push(block_pass);
            }
        }
        for forwarded_arg in forwarded_args {
            register_offense(cx, forwarded_arg, node);
        }
        register_offense(cx, last_argument, node);
    }
}

fn expected_block_forwarding_style(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    last_argument: Node<'_>,
) -> bool {
    if style(cx) == "anonymous" {
        !explicit_block_argument(last_argument)
            || use_kwarg_in_method_definition(node)
            || use_block_argument_as_local_variable(node, &last_argument.source()[1..])
    } else {
        !anonymous_block_argument(last_argument)
    }
}

fn block_argument_name_matched(block_pass_node: Node<'_>, last_argument: Node<'_>) -> bool {
    if block_pass_node.child(0).is_some_and(|c| c.is(NodeType::Sym)) {
        return false;
    }
    last_argument.source() == block_pass_node.source()
}

/// Ruby 3.3.0 had a bug where accessing an anonymous block argument inside
/// of a block was a syntax error in unambiguous cases
/// (https://bugs.ruby-lang.org/issues/20090). We disallow this also for
/// earlier Ruby versions so that code is forwards compatible.
fn invalidates_syntax(cx: &Investigation<'_, '_>, block_pass_node: Node<'_>) -> bool {
    cx.target_ruby_version() <= 3.3 && block_pass_node.ancestors().any(|a| a.is_any_block())
}

fn use_kwarg_in_method_definition(node: Node<'_>) -> bool {
    node.arguments_node().is_some_and(|args| {
        args.descendants()
            .any(|d| matches!(d.ty(), NodeType::Kwarg | NodeType::Kwoptarg))
    })
}

fn anonymous_block_argument(node: Node<'_>) -> bool {
    node.is(NodeType::Blockarg) && node.name().is_none()
}

fn explicit_block_argument(node: Node<'_>) -> bool {
    node.is(NodeType::Blockarg) && node.name().is_some()
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, block_argument: Node<'a>, node: Node<'a>) {
    let style = style(cx);
    let message = format!("Use {style} block forwarding.");
    let name = block_forwarding_name(cx);
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(block_argument.source_range(), &message, |corrector| {
        if style == "anonymous" {
            corrector.replace(block_argument.source_range(), "&");
            let arguments = block_argument.parent().unwrap();
            if !arguments.is_parenthesized_call() {
                add_parentheses(src, arguments, corrector);
            }
        } else if !use_block_argument_as_local_variable(node, name) {
            corrector.replace(block_argument.source_range(), format!("&{name}"));
        }
    });
}

fn use_block_argument_as_local_variable(node: Node<'_>, last_argument: &str) -> bool {
    let Some(body) = node.body() else {
        return false;
    };
    body.each_node()
        .filter(|n| matches!(n.ty(), NodeType::Lvar | NodeType::Lvasgn))
        .any(|lvar| {
            !lvar.parent().is_some_and(|p| p.is(NodeType::BlockPass))
                && lvar.name() == Some(last_argument)
        })
}

fn block_forwarding_name<'a>(cx: &Investigation<'a, '_>) -> &'a str {
    cx.cop_config_str("BlockForwardingName").unwrap_or("block")
}
