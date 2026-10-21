//! Port of `lib/rubocop/cop/style/super_arguments.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Call `super` without arguments and parentheses when the signature is identical.";
const MSG_INLINE_BLOCK: &str = "Call `super` without arguments and parentheses when all positional and keyword arguments are forwarded.";

#[derive(Default)]
pub struct SuperArguments;

impl Cop for SuperArguments {
    fn name(&self) -> &'static str {
        "Style/SuperArguments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Super]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, super_node: Node<'a>) {
        let Some(def_node) = find_def_node(super_node) else {
            return;
        };
        let def_node_args: Vec<Node<'a>> = def_node
            .arguments_node()
            .map(|args| args.descendants().filter(|d| d.is_argument_type()).collect())
            .unwrap_or_default();
        let super_args = preprocess_super_args(super_node.arguments());
        if !arguments_identical(def_node, super_node, &def_node_args, &super_args) {
            return;
        }
        let message = if def_node_args.len() == super_args.len() {
            MSG
        } else {
            MSG_INLINE_BLOCK
        };
        cx.add_offense_with_correction(super_node.source_range(), message, |corrector| {
            corrector.replace(super_node.source_range(), "super");
        });
    }
}

fn find_def_node(super_node: Node<'_>) -> Option<Node<'_>> {
    for node in super_node.ancestors() {
        if node.is_any_block() && !block_sends_to_super(super_node, Some(node)) {
            return None;
        }
        if node.is_any_def() {
            return Some(node);
        }
    }
    None
}

fn arguments_identical(
    def_node: Node<'_>,
    super_node: Node<'_>,
    def_args: &[Node<'_>],
    super_args: &[Node<'_>],
) -> bool {
    if argument_list_size_differs(def_args, super_args, super_node) {
        return false;
    }
    for (index, def_arg) in def_args.iter().enumerate() {
        let def_arg = *def_arg;
        // `zip` pads with `nil`; a missing super argument matches nothing
        // except a block argument forwarded by an inline block.
        let super_arg = super_args.get(index).copied();
        if let Some(super_arg) = super_arg
            && (positional_arg_same(def_arg, super_arg)
                || positional_rest_arg_same(def_arg, super_arg)
                || keyword_arg_same(def_arg, super_arg)
                || keyword_rest_arg_same(def_arg, super_arg)
                || forward_arg_same(def_arg, super_arg))
        {
            continue;
        }
        if block_arg_same(def_node, super_node, def_arg, super_arg) {
            continue;
        }
        return false;
    }
    true
}

fn argument_list_size_differs(
    def_args: &[Node<'_>],
    super_args: &[Node<'_>],
    super_node: Node<'_>,
) -> bool {
    let mut def_args_size = def_args.len();
    if def_args.iter().any(|a| a.is(NodeType::Blockarg))
        && block_sends_to_super(super_node, super_node.parent())
    {
        def_args_size -= 1;
    }
    def_args_size != super_args.len()
}

fn block_sends_to_super(super_node: Node<'_>, parent_node: Option<Node<'_>>) -> bool {
    let Some(parent_node) = parent_node else {
        return false;
    };
    if !parent_node.is_any_block() {
        return false;
    }
    parent_node.send_node().is_some_and(|send| {
        send.each_node()
            .any(|n| n.is(NodeType::Super) && n.same_as(super_node))
    })
}

fn positional_arg_same(def_arg: Node<'_>, super_arg: Node<'_>) -> bool {
    def_arg.is_any(&[NodeType::Arg, NodeType::Optarg])
        && super_arg.is(NodeType::Lvar)
        && def_arg.name().is_some()
        && def_arg.name() == super_arg.child_sym(0)
}

fn positional_rest_arg_same(def_arg: Node<'_>, super_arg: Node<'_>) -> bool {
    if !def_arg.is(NodeType::Restarg) {
        return false;
    }
    if def_arg.name().is_none() && super_arg.is(NodeType::ForwardedRestarg) {
        return true;
    }
    if !super_arg.is(NodeType::Splat) {
        return false;
    }
    let Some(lvar_node) = super_arg.child(0).filter(|n| n.is(NodeType::Lvar)) else {
        return false;
    };
    def_arg.name() == lvar_node.child_sym(0)
}

fn keyword_arg_same(def_arg: Node<'_>, super_arg: Node<'_>) -> bool {
    if !def_arg.is_any(&[NodeType::Kwarg, NodeType::Kwoptarg]) || !super_arg.is(NodeType::Pair) {
        return false;
    }
    let Some(sym_node) = super_arg.key().filter(|k| k.is(NodeType::Sym)) else {
        return false;
    };
    let Some(lvar_node) = super_arg.pair_value().filter(|v| v.is(NodeType::Lvar)) else {
        return false;
    };
    if sym_node.source() != lvar_node.source() {
        return false;
    }
    def_arg.name() == sym_node.sym_value()
}

fn keyword_rest_arg_same(def_arg: Node<'_>, super_arg: Node<'_>) -> bool {
    if !def_arg.is(NodeType::Kwrestarg) {
        return false;
    }
    if def_arg.name().is_none() && super_arg.is(NodeType::ForwardedKwrestarg) {
        return true;
    }
    if !super_arg.is(NodeType::Kwsplat) {
        return false;
    }
    let Some(lvar_node) = super_arg.child(0).filter(|n| n.is(NodeType::Lvar)) else {
        return false;
    };
    def_arg.name() == lvar_node.child_sym(0)
}

fn block_arg_same(
    def_node: Node<'_>,
    super_node: Node<'_>,
    def_arg: Node<'_>,
    super_arg: Option<Node<'_>>,
) -> bool {
    if !def_arg.is(NodeType::Blockarg) {
        return false;
    }
    if block_sends_to_super(super_node, super_node.parent()) {
        return true;
    }
    let Some(super_arg) = super_arg.filter(|a| a.is(NodeType::BlockPass)) else {
        return false;
    };
    let Some(block_pass_child) = super_arg.child(0) else {
        return def_arg.name().is_none();
    };
    let block_arg_name = block_pass_child.child_sym(0);
    block_arg_name.is_some()
        && def_arg.name() == block_arg_name
        && !block_reassigned(def_node, block_arg_name.unwrap())
}

fn block_reassigned(def_node: Node<'_>, block_arg_name: &str) -> bool {
    def_node
        .each_node()
        .filter(|n| n.is_any(&[NodeType::OrAsgn, NodeType::Lvasgn]))
        .any(|assign_node| {
            if assign_node.is(NodeType::OrAsgn)
                && assign_node
                    .child(0)
                    .is_some_and(|lhs| lhs.is_call() || lhs.is_any(&[NodeType::Index, NodeType::Indexasgn]))
            {
                return false;
            }
            assign_node.name() == Some(block_arg_name)
        })
}

fn forward_arg_same(def_arg: Node<'_>, super_arg: Node<'_>) -> bool {
    def_arg.is(NodeType::ForwardArg) && super_arg.is(NodeType::ForwardedArgs)
}

fn preprocess_super_args(super_args: Vec<Node<'_>>) -> Vec<Node<'_>> {
    super_args
        .into_iter()
        .flat_map(|node| {
            if node.is(NodeType::Hash) && !node.has_braces() {
                node.child_nodes().collect()
            } else {
                vec![node]
            }
        })
        .collect()
}
