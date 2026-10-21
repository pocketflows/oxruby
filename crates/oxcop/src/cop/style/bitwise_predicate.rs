//! Port of `lib/rubocop/cop/style/bitwise_predicate.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static ANYBITS: NodePattern = NodePattern::new(
    "{
       (send #bit_operation? :positive?)
       (send #bit_operation? :> (int 0))
       (send #bit_operation? :>= (int 1))
       (send #bit_operation? :!= (int 0))
     }",
);

static ALLBITS: NodePattern = NodePattern::new(
    "{
       (send (begin (send _ :& _flags)) :== _flags)
       (send (begin (send _flags :& _)) :== _flags)
     }",
);

static NOBITS: NodePattern = NodePattern::new(
    "{
       (send #bit_operation? :zero?)
       (send #bit_operation? :== (int 0))
     }",
);

static BIT_OPERATION: NodePattern = NodePattern::new(
    "(begin
       (send _ :& _))",
);

#[derive(Default)]
pub struct BitwisePredicate;

impl Cop for BitwisePredicate {
    fn name(&self) -> &'static str {
        "Style/BitwisePredicate"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.5)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!=", "==", ">", ">=", "positive?", "zero?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver().filter(|r| r.is(NodeType::Begin)) else {
            return;
        };
        let Some(preferred_method) = preferred_method(node) else {
            return;
        };
        let Some(bit_operation) = receiver.child(0) else {
            return;
        };
        let (Some(lhs), Some(rhs)) = (bit_operation.child(0), bit_operation.child(2)) else {
            return;
        };
        let preferred = if preferred_method == "allbits?"
            && node
                .first_argument()
                .is_some_and(|a| a.source() == lhs.source())
        {
            format!("{}.allbits?({})", rhs.source(), lhs.source())
        } else {
            format!("{}.{preferred_method}({})", lhs.source(), rhs.source())
        };
        let message = format!("Replace with `{preferred}` for comparison with bit flags.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), preferred);
        });
    }
}

fn preferred_method(node: Node<'_>) -> Option<&'static str> {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "bit_operation?" && val.node().is_some_and(|n| BIT_OPERATION.is_match(n))
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    if ANYBITS.match_with(node, &ctx).is_some() {
        Some("anybits?")
    } else if ALLBITS.is_match(node) {
        Some("allbits?")
    } else if NOBITS.match_with(node, &ctx).is_some() {
        Some("nobits?")
    } else {
        None
    }
}
