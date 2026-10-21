//! Port of `lib/rubocop/cop/style/array_coercion.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static ARRAY_SPLAT: NodePattern = NodePattern::new("(array (splat $_))");

static UNLESS_ARRAY: NodePattern = NodePattern::new(
    "(if
       (send
         (lvar $_) :is_a?
         (const nil? :Array)) nil?
       (lvasgn $_
         (array
           (lvar $_))))",
);

#[derive(Default)]
pub struct ArrayCoercion;

impl Cop for ArrayCoercion {
    fn name(&self) -> &'static str {
        "Style/ArrayCoercion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array, NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Array) {
            if !node.is_square_brackets() {
                return;
            }
            let Some(captures) = ARRAY_SPLAT.match_node(node) else {
                return;
            };
            let arg = captures[0].node().unwrap().source();
            let message = format!("Use `Array({arg})` instead of `[*{arg}]`.");
            cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
                corrector.replace(node.source_range(), format!("Array({arg})"));
            });
        } else {
            let Some(captures) = UNLESS_ARRAY.match_node(node) else {
                return;
            };
            let (var_a, var_b, var_c) = (captures[0].sym(), captures[1].sym(), captures[2].sym());
            if var_a != var_b || var_c != var_b {
                return;
            }
            let var = var_a.unwrap_or("");
            let message = format!("Use `Array({var})` instead of explicit `Array` check.");
            cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
                corrector.replace(node.source_range(), format!("{var} = Array({var})"));
            });
        }
    }
}
