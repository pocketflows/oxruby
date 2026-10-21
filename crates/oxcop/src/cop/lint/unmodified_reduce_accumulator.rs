//! Port of `lib/rubocop/cop/lint/unmodified_reduce_accumulator.rb`.

use crate::ast::node_pattern::{Capture, MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static REDUCE_WITH_BLOCK: NodePattern = NodePattern::new(
    "{
       (block (call _recv {:reduce :inject} ...) args ...)
       (numblock (call _recv {:reduce :inject} ...) ...)
     }",
);
static ACCUMULATOR_INDEX: NodePattern = NodePattern::new("(send (lvar %1) {:[] :[]=} ...)");
static ELEMENT_MODIFIED: NodePattern = NodePattern::new(
    "{
       (send _receiver !{:[] :[]=} <`(lvar %1) `_ ...>)               # method(el, ...)
       (send (lvar %1) _message <{ivar gvar cvar lvar send} ...>)     # el.method(...)
       (lvasgn %1 _)                                                  # el = ...
       (%RuboCop::AST::Node::SHORTHAND_ASSIGNMENTS (lvasgn %1) ... _) # el += ...
     }",
);
static LVAR_USED: NodePattern = NodePattern::new(
    "{
       (lvar %1)
       (lvasgn %1 ...)
       (send (lvar %1) :<< ...)
       (dstr (begin (lvar %1)))
       (%RuboCop::AST::Node::SHORTHAND_ASSIGNMENTS (lvasgn %1))
     }",
);
static EXPRESSION_VALUES: NodePattern = NodePattern::new(
    "{
       (%RuboCop::AST::Node::VARIABLES $_)
       (%RuboCop::AST::Node::EQUALS_ASSIGNMENTS $_ ...)
       (send (%RuboCop::AST::Node::VARIABLES $_) :<< ...)
       $(send _ _)
       (dstr (begin {(%RuboCop::AST::Node::VARIABLES $_)}))
       (%RuboCop::AST::Node::SHORTHAND_ASSIGNMENTS (%RuboCop::AST::Node::EQUALS_ASSIGNMENTS $_) ...)
     }",
);

const NAMED: &[(&str, Param<'static>)] = &[
    (
        "RuboCop::AST::Node::SHORTHAND_ASSIGNMENTS",
        Param::Syms(&["op_asgn", "or_asgn", "and_asgn"]),
    ),
    (
        "RuboCop::AST::Node::VARIABLES",
        Param::Syms(&["ivar", "gvar", "cvar", "lvar"]),
    ),
    (
        "RuboCop::AST::Node::EQUALS_ASSIGNMENTS",
        Param::Syms(&["lvasgn", "ivasgn", "cvasgn", "gvasgn", "casgn", "masgn"]),
    ),
];

fn with_name<R>(name: &str, f: impl FnOnce(&MatchContext<'_, '_>) -> R) -> R {
    let params = [Param::Sym(name)];
    let ctx = MatchContext {
        params: &params,
        named: NAMED,
        ..Default::default()
    };
    f(&ctx)
}

fn accumulator_index(node: Node<'_>, name: &str) -> bool {
    with_name(name, |ctx| ACCUMULATOR_INDEX.match_with(node, ctx).is_some())
}

fn element_modified(node: Node<'_>, name: &str) -> bool {
    with_name(name, |ctx| !ELEMENT_MODIFIED.search_with(node, ctx).is_empty())
}

fn lvar_used(node: Node<'_>, name: &str) -> bool {
    with_name(name, |ctx| LVAR_USED.match_with(node, ctx).is_some())
}

#[derive(Default)]
pub struct UnmodifiedReduceAccumulator;

impl Cop for UnmodifiedReduceAccumulator {
    fn name(&self) -> &'static str {
        "Lint/UnmodifiedReduceAccumulator"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.body().is_none() {
            return;
        }
        if !REDUCE_WITH_BLOCK.is_match(node) {
            return;
        }
        if node.block_argument_names().len() < 2 {
            return;
        }

        check_return_values(cx, node);
    }
}

fn return_values(block_body_node: Node<'_>) -> Vec<Node<'_>> {
    let mut nodes = Vec::new();
    if block_body_node.is(NodeType::Begin) {
        nodes.extend(block_body_node.child_nodes().next_back());
    } else {
        nodes.push(block_body_node);
    }
    let block = block_body_node.parent();
    for n in block_body_node
        .descendants()
        .filter(|d| matches!(d.ty(), NodeType::Next | NodeType::Break))
    {
        let first_block = n.ancestors().find(|a| a.is_any_block());
        let same = match (first_block, block) {
            (Some(a), Some(b)) => a.same_as(b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            continue;
        }
        let Some(first_argument) = n.first_argument() else {
            continue;
        };
        nodes.push(first_argument);
    }
    nodes
}

fn check_return_values(cx: &mut Investigation<'_, '_>, block_node: Node<'_>) {
    let Some(body) = block_node.body() else {
        return;
    };
    let return_values = return_values(body);
    let names = block_node.block_argument_names();
    let accumulator_name = names[0].as_str();
    let element_name = names[1].as_str();
    let method = block_node.method_name().unwrap_or_default();

    if let Some(node) = returned_accumulator_index(&return_values, accumulator_name, element_name)
    {
        let message = format!("Do not return an element of the accumulator in `{method}`.");
        cx.add_offense(node.source_range(), &message);
    } else if potential_offense(&return_values, body, element_name, accumulator_name) {
        let message =
            format!("Ensure the accumulator `{accumulator_name}` will be modified by `{method}`.");
        for return_val in &return_values {
            if !acceptable_return(*return_val, element_name) {
                cx.add_offense(return_val.source_range(), &message);
            }
        }
    }
}

fn returned_accumulator_index<'a>(
    return_values: &[Node<'a>],
    accumulator_name: &str,
    element_name: &str,
) -> Option<Node<'a>> {
    return_values.iter().copied().find(|val| {
        if !accumulator_index(*val, accumulator_name) {
            return false;
        }
        if val.is_method("[]=") {
            return true;
        }
        !val.arguments()
            .into_iter()
            .any(|arg| lvar_used(arg, element_name))
    })
}

fn potential_offense(
    return_values: &[Node<'_>],
    block_body: Node<'_>,
    element_name: &str,
    accumulator_name: &str,
) -> bool {
    !(element_modified(block_body, element_name)
        || returns_accumulator_anywhere(return_values, accumulator_name))
}

fn returns_accumulator_anywhere(return_values: &[Node<'_>], accumulator_name: &str) -> bool {
    return_values
        .iter()
        .any(|node| lvar_used(*node, accumulator_name))
}

fn acceptable_return(return_val: Node<'_>, element_name: &str) -> bool {
    let ctx = MatchContext {
        named: NAMED,
        ..Default::default()
    };
    let vars: Vec<Capture<'_>> = EXPRESSION_VALUES
        .search_with(return_val, &ctx)
        .into_iter()
        .flat_map(|(_, captures)| captures)
        .collect();
    // `vars.none?`: no truthy values.
    let truthy: Vec<&Capture<'_>> = vars.iter().filter(|v| !v.is_nil()).collect();
    if truthy.is_empty() {
        return true;
    }
    // `(vars - [element_name]).any?`
    truthy.iter().any(|v| v.sym() != Some(element_name))
}
