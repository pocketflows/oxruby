//! Port of `lib/rubocop/cop/style/swap_values.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{range_between, range_by_whole_lines};
use crate::cop::{Cop, Investigation};

const SIMPLE_ASSIGNMENT_TYPES: &[NodeType] = &[
    NodeType::Lvasgn,
    NodeType::Ivasgn,
    NodeType::Cvasgn,
    NodeType::Gvasgn,
    NodeType::Casgn,
];

#[derive(Default)]
pub struct SwapValues;

impl Cop for SwapValues {
    fn name(&self) -> &'static str {
        "Style/SwapValues"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        SIMPLE_ASSIGNMENT_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if allowed_assignment(node) {
            return;
        }
        let tmp_assign = node;
        let (Some(parent), Some(index)) = (node.parent(), node.sibling_index()) else {
            return;
        };
        // `right_siblings.take(2)`, values included.
        let (Some(x_assign), Some(y_assign)) = (parent.child(index + 1), parent.child(index + 2))
        else {
            return;
        };
        if !swapping_values(tmp_assign, x_assign, y_assign) {
            return;
        }
        let message = format!(
            "Replace this and assignments at lines {} and {} with `{}`.",
            x_assign.first_line(),
            y_assign.first_line(),
            replacement(x_assign)
        );
        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let range = range_by_whole_lines(
                ps,
                range_between(
                    tmp_assign.source_range().begin_pos,
                    y_assign.source_range().end_pos,
                ),
                false,
            );
            corrector.replace(range, replacement(x_assign));
        });
    }
}

fn allowed_assignment(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::Mlhs) || p.is_shorthand_asgn())
}

fn swapping_values(tmp_assign: Node<'_>, x_assign: Node<'_>, y_assign: Node<'_>) -> bool {
    simple_assignment(tmp_assign)
        && simple_assignment(x_assign)
        && simple_assignment(y_assign)
        && Some(lhs(x_assign)) == rhs(tmp_assign).map(str::to_owned)
        && Some(lhs(y_assign)) == rhs(x_assign).map(str::to_owned)
        && rhs(y_assign).map(str::to_owned) == Some(lhs(tmp_assign))
}

fn simple_assignment(node: Node<'_>) -> bool {
    node.is_any(SIMPLE_ASSIGNMENT_TYPES)
}

fn replacement(x_assign: Node<'_>) -> String {
    let x = lhs(x_assign);
    let y = rhs(x_assign).unwrap_or_default();
    format!("{x}, {y} = {y}, {x}")
}

fn lhs(node: Node<'_>) -> String {
    if node.is(NodeType::Casgn) {
        format!(
            "{}{}",
            if node.is_absolute() { "::" } else { "" },
            node.const_name().unwrap_or_default()
        )
    } else {
        node.name().unwrap_or_default().to_owned()
    }
}

fn rhs(node: Node<'_>) -> Option<&str> {
    node.expression().map(|e| e.source())
}
