//! Port of `lib/rubocop/cop/lint/useless_constant_scoping.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Useless `private` access modifier for constant scope.";

static PRIVATE_CONSTANTS: NodePattern = NodePattern::new("(send nil? :private_constant $...)");
static CLASS_OR_MODULE_DEFINITION_ASSIGNMENT: NodePattern = NodePattern::new(
    "{
       (send (const {nil? cbase} {:Class :Module :Struct}) :new ...)
       (send (const {nil? cbase} :Data) :define ...)
       (any_block
         {
           (send (const {nil? cbase} {:Class :Module :Struct}) :new ...)
           (send (const {nil? cbase} :Data) :define ...)
         }
         ...)
     }",
);

#[derive(Default)]
pub struct UselessConstantScoping;

impl Cop for UselessConstantScoping {
    fn name(&self) -> &'static str {
        "Lint/UselessConstantScoping"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Casgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !after_private_modifier(&node.left_siblings()) {
            return;
        }
        if private_constantize(&node.right_siblings(), node.short_name().unwrap_or_default()) {
            return;
        }
        if node
            .expression()
            .is_some_and(|e| CLASS_OR_MODULE_DEFINITION_ASSIGNMENT.is_match(e))
        {
            return;
        }

        cx.add_offense(node.source_range(), MSG);
    }
}

fn after_private_modifier(left_siblings: &[Node<'_>]) -> bool {
    let access_modifier_candidates: Vec<Node<'_>> = left_siblings
        .iter()
        .copied()
        .filter(|left_sibling| left_sibling.is_send() && left_sibling.is_bare_access_modifier())
        .collect();
    let Some(last) = access_modifier_candidates.last() else {
        return false;
    };

    last.is_command("private")
}

fn private_constantize(right_siblings: &[Node<'_>], const_value: &str) -> bool {
    right_siblings.iter().any(|node| {
        PRIVATE_CONSTANTS.match_node(*node).is_some_and(|captures| {
            captures[0].nodes().iter().any(|constant| {
                (constant.is(NodeType::Sym) || constant.is(NodeType::Str))
                    && (constant.sym_value().or(constant.str_content()) == Some(const_value))
            })
        })
    })
}
