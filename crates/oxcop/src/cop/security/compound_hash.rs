//! Port of `lib/rubocop/cop/security/compound_hash.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const COMBINATOR_IN_HASH_MSG: &str = "Use `[...].hash` instead of combining hash values manually.";
const MONUPLE_HASH_MSG: &str =
    "Delegate hash directly without wrapping in an array when only using a single value.";
const REDUNDANT_HASH_MSG: &str = "Calling .hash on elements of a hashed array is redundant.";

static HASH_METHOD_DEFINITION: NodePattern = NodePattern::new(
    "{#static_hash_method_definition? | #dynamic_hash_method_definition?}",
);

static DYNAMIC_HASH_METHOD_DEFINITION: NodePattern = NodePattern::new(
    "(block
       (send _ {:define_method | :define_singleton_method}
         (sym :hash))
       (args)
       _)",
);

static STATIC_HASH_METHOD_DEFINITION: NodePattern = NodePattern::new(
    "({def | defs _} :hash
       (args)
       _)",
);

static BAD_HASH_COMBINATOR: NodePattern =
    NodePattern::new("({send | op-asgn} _ {:^ | :+ | :* | :|} _)");

static MONUPLE_HASH: NodePattern = NodePattern::new("(send (array _) :hash)");

static REDUNDANT_HASH: NodePattern = NodePattern::new(
    "(
       ^^(send array ... :hash)
       _ :hash
     )",
);

fn hash_method_definition(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        let Some(node) = val.node() else {
            return false;
        };
        match name {
            "static_hash_method_definition?" => STATIC_HASH_METHOD_DEFINITION.is_match(node),
            "dynamic_hash_method_definition?" => DYNAMIC_HASH_METHOD_DEFINITION.is_match(node),
            _ => unreachable!(),
        }
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    HASH_METHOD_DEFINITION.match_with(node, &ctx).is_some()
}

fn contained_in_hash_method(node: Node<'_>) -> bool {
    node.ancestors().any(hash_method_definition)
}

fn outer_bad_hash_combinator(node: Node<'_>) -> bool {
    BAD_HASH_COMBINATOR.is_match(node) && node.ancestors().all(|a| !BAD_HASH_COMBINATOR.is_match(a))
}

#[derive(Default)]
pub struct CompoundHash;

impl Cop for CompoundHash {
    fn name(&self) -> &'static str {
        "Security/CompoundHash"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::OpAsgn]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["hash", "^", "+", "*", "|"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if outer_bad_hash_combinator(node) && contained_in_hash_method(node) {
            cx.add_offense(node.source_range(), COMBINATOR_IN_HASH_MSG);
        }
        if MONUPLE_HASH.is_match(node) {
            cx.add_offense(node.source_range(), MONUPLE_HASH_MSG);
        }
        if REDUNDANT_HASH.is_match(node) {
            cx.add_offense(node.source_range(), REDUNDANT_HASH_MSG);
        }
    }
}
