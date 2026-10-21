//! Port of `lib/rubocop/cop/lint/non_deterministic_require_order.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Sort files before requiring them.";

static UNSORTED_DIR_BLOCK: NodePattern = NodePattern::new("(send (const {nil? cbase} :Dir) :glob ...)");
static UNSORTED_DIR_EACH: NodePattern =
    NodePattern::new("(send (send (const {nil? cbase} :Dir) {:[] :glob} ...) :each)");
static METHOD_REQUIRE: NodePattern =
    NodePattern::new("(block-pass (send nil? :method (sym {:require :require_relative})))");
static UNSORTED_DIR_GLOB_PASS: NodePattern = NodePattern::new(
    "(send (const {nil? cbase} :Dir) :glob ...
      (block-pass (send nil? :method (sym {:require :require_relative}))))",
);
static UNSORTED_DIR_EACH_PASS: NodePattern = NodePattern::new(
    "(send (send (const {nil? cbase} :Dir) {:[] :glob} ...) :each
      (block-pass (send nil? :method (sym {:require :require_relative}))))",
);
static LOOP_VARIABLE: NodePattern = NodePattern::new("(args (arg $_))");
static VAR_IS_REQUIRED: NodePattern =
    NodePattern::new("(send nil? {:require :require_relative} (lvar %1))");

#[derive(Default)]
pub struct NonDeterministicRequireOrder;

impl Cop for NonDeterministicRequireOrder {
    fn name(&self) -> &'static str {
        "Lint/NonDeterministicRequireOrder"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn maximum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::BlockPass]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Block => on_block(cx, node),
            NodeType::Numblock => on_numblock(cx, node),
            _ => on_block_pass(cx, node),
        }
    }
}

fn on_block<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(body) = node.body() else {
        return;
    };
    let send_node = node.send_node().unwrap();
    if !unsorted_dir_loop(send_node) {
        return;
    }
    let Some(var_name) = node
        .child(1)
        .and_then(|args| LOOP_VARIABLE.match_node(args))
        .and_then(|c| c[0].sym())
    else {
        return;
    };
    if !var_is_required(body, var_name) {
        return;
    }
    cx.add_offense_with_correction(send_node.source_range(), MSG, |corrector| {
        correct_block(corrector, send_node);
    });
}

fn on_numblock<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(body) = node.body() else {
        return;
    };
    let send_node = node.send_node().unwrap();
    if !unsorted_dir_loop(send_node) {
        return;
    }
    // `node.argument_list`: `_1` up to the highest numbered parameter.
    let max = node.child(1).and_then(|c| c.int_value()).or(match node.child_value(1) {
        Some(crate::ast::Child::Int(i)) => Some(*i),
        _ => None,
    });
    for n in 1..=max.unwrap_or(0) {
        let name = format!("_{n}");
        if var_is_required(body, &name) {
            cx.add_offense_with_correction(send_node.source_range(), MSG, |corrector| {
                correct_block(corrector, send_node);
            });
        }
    }
}

fn on_block_pass<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if !METHOD_REQUIRE.is_match(node) {
        return;
    }
    let Some(parent_node) = node.parent() else {
        return;
    };
    if !unsorted_dir_pass(parent_node) {
        return;
    }
    cx.add_offense_with_correction(parent_node.source_range(), MSG, |corrector| {
        if parent_node
            .last_argument()
            .is_some_and(|a| a.is(NodeType::BlockPass))
        {
            correct_block_pass(corrector, parent_node);
        } else {
            correct_block(corrector, parent_node);
        }
    });
}

fn correct_block(corrector: &mut Corrector, node: Node<'_>) {
    if UNSORTED_DIR_BLOCK.is_match(node) {
        corrector.replace(node.source_range(), format!("{}.sort.each", node.source()));
    } else {
        let source = node.receiver().unwrap().source();
        corrector.replace(node.source_range(), format!("{source}.sort.each"));
    }
}

fn correct_block_pass(corrector: &mut Corrector, node: Node<'_>) {
    if UNSORTED_DIR_GLOB_PASS.is_match(node) {
        let block_arg = node.last_argument().unwrap();
        corrector.remove(last_arg_range(node));
        corrector.insert_after(
            node.source_range(),
            format!(".sort.each({})", block_arg.source()),
        );
    } else {
        corrector.replace(node.loc().selector.unwrap(), "sort.each");
    }
}

fn last_arg_range(node: Node<'_>) -> Range {
    let arguments = node.arguments();
    node.last_argument()
        .unwrap()
        .source_range()
        .join(arguments[arguments.len() - 2].source_range().end())
}

fn unsorted_dir_loop(node: Node<'_>) -> bool {
    UNSORTED_DIR_BLOCK.is_match(node) || UNSORTED_DIR_EACH.is_match(node)
}

fn unsorted_dir_pass(node: Node<'_>) -> bool {
    UNSORTED_DIR_GLOB_PASS.is_match(node) || UNSORTED_DIR_EACH_PASS.is_match(node)
}

fn var_is_required(body: Node<'_>, var_name: &str) -> bool {
    let params = [Param::Sym(var_name)];
    let ctx = MatchContext {
        params: &params,
        ..Default::default()
    };
    body.each_node()
        .any(|n| VAR_IS_REQUIRED.match_with(n, &ctx).is_some())
}
