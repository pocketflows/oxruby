//! Port of `lib/rubocop/cop/style/redundant_initialize.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::comments_help::{comments_contain_disables, contains_comments};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove unnecessary `initialize` method.";
const MSG_EMPTY: &str = "Remove unnecessary empty `initialize` method.";

#[derive(Default)]
pub struct RedundantInitialize;

impl Cop for RedundantInitialize {
    fn name(&self) -> &'static str {
        "Style/RedundantInitialize"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if acceptable(cx, node) {
            return;
        }
        match node.body() {
            None => {
                if node.arguments().is_empty() {
                    register_offense(cx, node, MSG_EMPTY);
                }
            }
            Some(body) => {
                if body.is(NodeType::Begin) {
                    return;
                }
                // (def _ (args $arg*) $({super zsuper} ...))
                let args = node.arguments();
                if !args.iter().all(|a| a.is(NodeType::Arg))
                    || !matches!(body.ty(), NodeType::Super | NodeType::Zsuper)
                {
                    return;
                }
                if !same_args(body, &args) {
                    return;
                }
                register_offense(cx, node, MSG);
            }
        }
    }
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, message: &str) {
    let ps = cx.processed_source;
    cx.add_offense_with_correction(node.source_range(), message, |corrector| {
        corrector.remove(range_by_whole_lines(ps, node.source_range(), true));
    });
}

fn acceptable(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    !node.is_method("initialize") || forwards(node) || allow_comments(cx, node)
}

fn forwards(node: Node<'_>) -> bool {
    node.arguments().iter().any(|a| {
        matches!(
            a.ty(),
            NodeType::Restarg | NodeType::Kwrestarg | NodeType::ForwardArgs | NodeType::ForwardArg
        )
    })
}

fn allow_comments(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !cx.cop_config_bool("AllowComments", false) {
        return false;
    }
    contains_comments(cx.processed_source, node)
        && !comments_contain_disables(cx, node, cx.cop_name())
}

fn same_args(super_node: Node<'_>, args: &[Node<'_>]) -> bool {
    if super_node.is(NodeType::Zsuper) {
        return true;
    }
    let super_args = super_node.arguments();
    args.len() == super_args.len()
        && args
            .iter()
            .zip(&super_args)
            .all(|(a, s)| s.child_sym(0).is_some() && a.name() == s.child_sym(0))
}
