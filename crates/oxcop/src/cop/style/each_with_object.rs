//! Port of `lib/rubocop/cop/style/each_with_object.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

static EACH_WITH_OBJECT_BLOCK_CANDIDATE: NodePattern =
    NodePattern::new("(block $(call _ {:inject :reduce} _) $(args _ _) $_)");

static EACH_WITH_OBJECT_NUMBLOCK_CANDIDATE: NodePattern =
    NodePattern::new("(numblock $(call _ {:inject :reduce} _) 2 $_)");

#[derive(Default)]
pub struct EachWithObject;

impl Cop for EachWithObject {
    fn name(&self) -> &'static str {
        "Style/EachWithObject"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Itblock, NodeType::Numblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Numblock) {
            self.on_numblock(cx, node);
            return;
        }
        let Some(captures) = EACH_WITH_OBJECT_BLOCK_CANDIDATE.match_node(node) else {
            return;
        };
        let method = captures[0].node().unwrap();
        let args = captures[1].node().unwrap();
        let body = captures[2].node();
        if simple_method_arg(method.child(2)) {
            return;
        }
        let Some(return_value) = return_value(body) else {
            return;
        };
        if !first_argument_returned(args, return_value) {
            return;
        }
        if accumulator_param_assigned_to(body, args) {
            return;
        }
        let message = format!(
            "Use `each_with_object` instead of `{}`.",
            method.method_name().unwrap_or("")
        );
        let ps = cx.processed_source;
        cx.add_offense_with_correction(method.loc().selector.unwrap(), &message, |corrector| {
            corrector.replace(
                node.send_node().unwrap().loc().selector.unwrap(),
                "each_with_object",
            );
            let arguments = node.arguments();
            corrector.swap(
                ps.buffer_source(),
                arguments[0].source_range(),
                arguments[1].source_range(),
            );
            if return_value_occupies_whole_line(ps, return_value) {
                corrector.remove(range_by_whole_lines(ps, return_value.source_range(), true));
            } else {
                corrector.remove(return_value.source_range());
            }
        });
    }
}

impl EachWithObject {
    fn on_numblock<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = EACH_WITH_OBJECT_NUMBLOCK_CANDIDATE.match_node(node) else {
            return;
        };
        let method = captures[0].node().unwrap();
        let body = captures[1].node();
        if simple_method_arg(method.child(2)) {
            return;
        }
        if return_value(body).map(|r| r.source()) != Some("_1") {
            return;
        }
        let message = format!(
            "Use `each_with_object` instead of `{}`.",
            method.method_name().unwrap_or("")
        );
        cx.add_offense_with_correction(method.loc().selector.unwrap(), &message, |corrector| {
            corrector.replace(
                node.send_node().unwrap().loc().selector.unwrap(),
                "each_with_object",
            );
            let Some(body) = node.body() else {
                return;
            };
            for var in body.descendants().filter(|d| d.is(NodeType::Lvar)) {
                if var.source() == "_1" {
                    corrector.replace(var.source_range(), "_2");
                }
                if var.source() == "_2" {
                    corrector.replace(var.source_range(), "_1");
                }
            }
        });
    }
}

fn simple_method_arg(method_arg: Option<Node<'_>>) -> bool {
    method_arg.is_some_and(|a| a.is_basic_literal())
}

/// The accumulator's name: the symbol of the first block argument.
fn accumulator_var(args: Node<'_>) -> Option<&Child> {
    args.child(0).and_then(|first| first.child_value(0))
}

fn accumulator_param_assigned_to(body: Option<Node<'_>>, args: Node<'_>) -> bool {
    let Some(body) = body else {
        return false;
    };
    let Some(Child::Sym(accumulator)) = accumulator_var(args) else {
        return false;
    };
    body.descendants().any(|n| {
        matches!(
            n.ty(),
            NodeType::Lvasgn | NodeType::Ivasgn | NodeType::Cvasgn | NodeType::Gvasgn
        ) && n.child_sym(0) == Some(accumulator.as_str())
    })
}

fn return_value(body: Option<Node<'_>>) -> Option<Node<'_>> {
    let body = body?;
    let return_value = if body.is(NodeType::Begin) {
        body.child_nodes().last()?
    } else {
        body
    };
    return_value.is(NodeType::Lvar).then_some(return_value)
}

fn first_argument_returned(args: Node<'_>, return_value: Node<'_>) -> bool {
    match (accumulator_var(args), return_value.child_value(0)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn return_value_occupies_whole_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    ps.slice(range_by_whole_lines(ps, node.source_range(), true))
        .trim()
        == node.source()
}
