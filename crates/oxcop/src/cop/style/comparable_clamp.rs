//! Port of `lib/rubocop/cop/style/comparable_clamp.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::indentation;
use crate::cop::{Cop, Investigation};

const MSG_MIN_MAX: &str = "Use `Comparable#clamp` instead.";

static IF_ELSIF_ELSE_CONDITION: NodePattern = NodePattern::new(
    "{
       (if (send _x :< _min) _min (if (send _max :< _x) _max _x))
       (if (send _min :> _x) _min (if (send _max :< _x) _max _x))
       (if (send _x :< _min) _min (if (send _x :> _max) _max _x))
       (if (send _min :> _x) _min (if (send _x :> _max) _max _x))
       (if (send _max :< _x) _max (if (send _x :< _min) _min _x))
       (if (send _x :> _max) _max (if (send _x :< _min) _min _x))
       (if (send _max :< _x) _max (if (send _min :> _x) _min _x))
       (if (send _x :> _max) _max (if (send _min :> _x) _min _x))
     }",
);

static ARRAY_MIN_MAX: NodePattern = NodePattern::new(
    "{
       (send
         (array
           (send (array _ _) :max) _) :min)
       (send
         (array
           _ (send (array _ _) :max)) :min)
       (send
         (array
           (send (array _ _) :min) _) :max)
       (send
         (array
           _ (send (array _ _) :min)) :max)
     }",
);

#[derive(Default)]
pub struct ComparableClamp;

impl Cop for ComparableClamp {
    fn name(&self) -> &'static str {
        "Style/ComparableClamp"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.4)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["min", "max"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_send() {
            if ARRAY_MIN_MAX.is_match(node) {
                cx.add_offense(node.source_range(), MSG_MIN_MAX);
            }
            return;
        }
        if !IF_ELSIF_ELSE_CONDITION.is_match(node) {
            return;
        }
        let branches = node.branches();
        let (Some(Some(if_body)), Some(Some(elsif_body)), Some(Some(else_body))) =
            (branches.first(), branches.get(1), branches.get(2))
        else {
            return;
        };
        let (min, max) = if min_condition(node.condition().unwrap(), else_body.source()) {
            (if_body.source(), elsif_body.source())
        } else {
            (elsif_body.source(), if_body.source())
        };
        let prefer = format!("{}.clamp({min}, {max})", parenthesize_if_needed(*else_body));
        let message = format!("Use `{prefer}` instead of `if/elsif/else`.");
        let indentation = indentation(cx, node);
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if node.is_elsif() {
                corrector.insert_before(node.source_range(), "else\n");
                corrector.replace(node.source_range(), format!("{indentation}{prefer}"));
            } else {
                corrector.replace(node.source_range(), prefer);
            }
        });
    }
}

fn min_condition(if_condition: Node<'_>, else_body: &str) -> bool {
    let (Some(lhs), Some(op), Some(rhs)) = (
        if_condition.child(0),
        if_condition.child_sym(1),
        if_condition.child(2),
    ) else {
        return false;
    };
    (lhs.source() == else_body && op == "<") || (rhs.source() == else_body && op == ">")
}

fn parenthesize_if_needed(node: Node<'_>) -> String {
    let assignment = if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    };
    if matches!(node.ty(), NodeType::And | NodeType::Or | NodeType::If)
        || node.is_range()
        || assignment
        || (node.is_send() && (node.is_operator_method() || node.is_unary_operation()))
    {
        format!("({})", node.source())
    } else {
        node.source().to_owned()
    }
}
