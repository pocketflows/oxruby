//! Port of `lib/rubocop/cop/style/redundant_freeze.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::frozen_string_literal::frozen_string_literal;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not freeze immutable objects, as freezing them has no effect.";

static OPERATION_PRODUCES_IMMUTABLE_OBJECT: NodePattern = NodePattern::new(
    "{
      (begin (send {float int} {:+ :- :* :** :/ :% :<<} _))
      (begin (send !{(str _) array} {:+ :- :* :** :/ :%} {float int}))
      (begin (send _ {:== :=== :!= :<= :>= :< :>} _))
      (send _ {:count :length :size} ...)
      (any_block (send _ {:count :length :size} ...) ...)
    }",
);

#[derive(Default)]
pub struct RedundantFreeze;

impl Cop for RedundantFreeze {
    fn name(&self) -> &'static str {
        "Style/RedundantFreeze"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["freeze"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        if !(immutable_literal(cx, receiver)
            || OPERATION_PRODUCES_IMMUTABLE_OBJECT.is_match(receiver))
        {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.remove(node.loc().dot.unwrap());
            corrector.remove(node.loc().selector.unwrap());
        });
    }
}

fn immutable_literal(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let node = match node.child(0) {
        Some(first) if node.is(NodeType::Begin) => first,
        _ => node,
    };
    if node.is_immutable_literal() || frozen_string_literal(cx, node) {
        return true;
    }
    cx.target_ruby_version() >= 3.0
        && matches!(
            node.ty(),
            NodeType::Regexp | NodeType::Irange | NodeType::Erange
        )
}
