//! Port of `lib/rubocop/cop/lint/useless_or.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const TRUTHY_RETURN_VALUE_METHODS: &[&str] = &[
    "to_a", "to_c", "to_d", "to_i", "to_f", "to_h", "to_r", "to_s", "to_sym", "intern", "inspect",
    "hash", "object_id", "__id__",
];

static TRUTHY_RETURN_VALUE_METHOD: NodePattern =
    NodePattern::new("(send _ %TRUTHY_RETURN_VALUE_METHODS)");

#[derive(Default)]
pub struct UselessOr;

impl Cop for UselessOr {
    fn name(&self) -> &'static str {
        "Lint/UselessOr"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Or]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let (Some(lhs), Some(rhs)) = (node.lhs(), node.rhs()) else {
            return;
        };
        if truthy_return_value_method(lhs) {
            report_offense(cx, node, lhs);
        } else if truthy_return_value_method(rhs) {
            let mut parent = node.parent();
            if let Some(p) = parent
                && p.is(NodeType::Begin)
            {
                parent = p.parent();
            }
            if let Some(parent) = parent
                && parent.is(NodeType::Or)
            {
                report_offense(cx, parent, rhs);
            }
        }
    }
}

fn truthy_return_value_method(node: Node<'_>) -> bool {
    let named = [(
        "TRUTHY_RETURN_VALUE_METHODS",
        Param::Syms(TRUTHY_RETURN_VALUE_METHODS),
    )];
    let ctx = MatchContext {
        named: &named,
        ..Default::default()
    };
    TRUTHY_RETURN_VALUE_METHOD.match_with(node, &ctx).is_some()
}

fn report_offense(cx: &mut Investigation<'_, '_>, or_node: Node<'_>, truthy_node: Node<'_>) {
    let (Some(operator), Some(lhs), Some(rhs)) =
        (or_node.loc().operator, or_node.lhs(), or_node.rhs())
    else {
        return;
    };
    let message = format!(
        "`{}` will never evaluate because `{}` always returns a truthy value.",
        rhs.source(),
        truthy_node.source()
    );
    cx.add_offense_with_correction(operator.join(rhs.source_range()), &message, |corrector| {
        corrector.replace(or_node.source_range(), lhs.source());
    });
}
