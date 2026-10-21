//! Port of `lib/rubocop/cop/style/not.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `!` instead of `not`.";

fn opposite_method(name: &str) -> Option<&'static str> {
    Some(match name {
        "==" => "!=",
        "!=" => "==",
        "<=" => ">",
        ">" => "<=",
        "<" => ">=",
        ">=" => "<",
        _ => return None,
    })
}

#[derive(Default)]
pub struct Not;

impl Cop for Not {
    fn name(&self) -> &'static str {
        "Style/Not"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_prefix_not() {
            return;
        }
        let src = cx.processed_source.buffer_source();
        let selector = node.loc().selector.unwrap();
        cx.add_offense_with_correction(selector, MSG, |corrector| {
            let range = range_with_surrounding_space(src, selector, SurroundingSpace::side(Side::Right));
            let receiver = node.receiver().unwrap();
            if is_opposite_method(receiver) {
                corrector.remove(range);
                corrector.replace(
                    receiver.loc().selector.unwrap(),
                    opposite_method(receiver.method_name().unwrap()).unwrap(),
                );
            } else if requires_parens(receiver) {
                corrector.replace(range, "!(");
                corrector.insert_after(node.source_range(), ")");
            } else {
                corrector.replace(range, "!");
            }
        });
    }
}

fn is_opposite_method(child: Node<'_>) -> bool {
    child.is_send() && child.method_name().and_then(opposite_method).is_some()
}

fn requires_parens(child: Node<'_>) -> bool {
    child.is_operator_keyword()
        || (child.is_send() && child.is_binary_operation())
        || (child.is(NodeType::If) && child.is_ternary())
        || matches!(
            child.ty(),
            NodeType::Irange | NodeType::Erange | NodeType::Iflipflop | NodeType::Eflipflop
        )
        || child.is_assignment()
}
