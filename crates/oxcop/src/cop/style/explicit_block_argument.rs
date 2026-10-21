//! Port of `lib/rubocop/cop/style/explicit_block_argument.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeId, NodeType};
use crate::cop::mixin::range_help::{Side, range_between, range_with_surrounding_comma};
use crate::cop::mixin::util::{args_begin, args_end};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str =
    "Consider using explicit block argument in the surrounding method's signature over `yield`.";

static YIELDING_BLOCK: NodePattern = NodePattern::new("(block $_ (args $...) (yield $...))");

#[derive(Default)]
pub struct ExplicitBlockArgument {
    /// `@def_nodes`: defs that already received a block argument in this
    /// investigation's corrections.
    def_nodes: Vec<NodeId>,
}

impl Cop for ExplicitBlockArgument {
    fn name(&self) -> &'static str {
        "Style/ExplicitBlockArgument"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Lint/UnusedMethodArgument"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Yield]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.def_nodes.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(block_node) = node.parent() else {
            return;
        };
        let Some(captures) = YIELDING_BLOCK.match_node(block_node) else {
            return;
        };
        let send_node = captures[0].node().unwrap();
        let block_args = captures[1].nodes();
        let yield_args = captures[2].nodes();
        if !yielding_arguments(&block_args, &yield_args) {
            return;
        }
        let Some(def_node) = block_node.ancestors().find(|a| a.is_any_def()) else {
            return;
        };
        let block_name = extract_block_name(def_node);
        let src = cx.processed_source.buffer_source();
        let def_nodes = &mut self.def_nodes;
        cx.add_offense_with_correction(block_node.source_range(), MSG, |corrector| {
            corrector.remove(block_body_range(block_node, send_node));
            add_block_argument(src, send_node, corrector, &block_name);
            if !def_nodes.contains(&def_node.id()) {
                def_nodes.push(def_node.id());
                add_block_argument(src, def_node, corrector, &block_name);
            }
        });
    }
}

fn extract_block_name(def_node: Node<'_>) -> String {
    if def_node.is_block_argument() {
        def_node
            .last_argument()
            .and_then(|a| a.name())
            .unwrap_or("")
            .to_owned()
    } else {
        "block".to_owned()
    }
}

fn first_child_eq(a: Node<'_>, b: Node<'_>) -> bool {
    match (a.children().first(), b.children().first()) {
        (Some(Child::Node(x)), Some(Child::Node(y))) => {
            a.ast().node(*x).same_as(b.ast().node(*y))
        }
        (x, y) => x == y,
    }
}

fn yielding_arguments(block_args: &[Node<'_>], yield_args: &[Node<'_>]) -> bool {
    let len = yield_args.len().max(block_args.len());
    (0..len).all(|i| match (yield_args.get(i), block_args.get(i)) {
        (Some(yield_arg), Some(block_arg)) => first_child_eq(*yield_arg, *block_arg),
        _ => false,
    })
}

fn add_block_argument(src: &str, node: Node<'_>, corrector: &mut Corrector, block_name: &str) {
    if node.has_arguments() {
        insert_argument(src, node, corrector, block_name);
    } else if let Some(args) = node
        .arguments_node()
        .filter(|a| a.is(NodeType::Args) && a.loc().begin.is_some())
        .filter(|_| node.is_any_def())
    {
        corrector.replace(args.source_range(), format!("(&{block_name})"));
    } else if node.is_call() || matches!(node.ty(), NodeType::Zsuper | NodeType::Super) {
        correct_call_node(node, corrector, block_name);
    } else {
        corrector.insert_after(node.loc().name.unwrap(), format!("(&{block_name})"));
    }
}

fn insert_argument(src: &str, node: Node<'_>, corrector: &mut Corrector, block_name: &str) {
    let last_arg = node.last_argument().unwrap();
    let arg_range = range_with_surrounding_comma(src, last_arg.source_range(), Side::Right);
    let mut replacement = format!(" &{block_name}");
    if !src[arg_range.begin_pos..arg_range.end_pos].ends_with(',') {
        replacement = format!(",{replacement}");
    }
    if !last_arg.is(NodeType::Blockarg) {
        corrector.insert_after(arg_range, replacement);
    }
}

fn correct_call_node(node: Node<'_>, corrector: &mut Corrector, block_name: &str) {
    let new_arguments = if node.is(NodeType::Zsuper) {
        let mut args = build_new_arguments_for_zsuper(node);
        args.push(format!("&{block_name}"));
        args.join(", ")
    } else {
        format!("&{block_name}")
    };
    corrector.insert_after(node.source_range(), format!("({new_arguments})"));
    if !node.is_parenthesized() {
        return;
    }
    let range = range_between(args_begin(node).begin_pos, args_end(node).end_pos);
    corrector.remove(range);
}

fn build_new_arguments_for_zsuper(node: Node<'_>) -> Vec<String> {
    let Some(def_node) = node.ancestors().find(|a| a.is_any_def()) else {
        return Vec::new();
    };
    def_node
        .arguments()
        .iter()
        .map(|arg| {
            if arg.is(NodeType::Optarg) {
                arg.name().unwrap_or("").to_owned()
            } else {
                arg.source().to_owned()
            }
        })
        .collect()
}

fn block_body_range(block_node: Node<'_>, send_node: Node<'_>) -> Range {
    range_between(
        send_node.source_range().end_pos,
        block_node.loc().end.unwrap().end_pos,
    )
}
