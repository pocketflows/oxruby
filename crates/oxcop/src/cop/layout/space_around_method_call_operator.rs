//! Port of `lib/rubocop/cop/layout/space_around_method_call_operator.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Avoid using spaces around a method call operator.";

#[derive(Default)]
pub struct SpaceAroundMethodCallOperator;

impl Cop for SpaceAroundMethodCallOperator {
    fn name(&self) -> &'static str {
        "Layout/SpaceAroundMethodCallOperator"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Const) {
            if node.loc().double_colon.is_none() {
                return;
            }
            check_space_after_double_colon(cx, node);
            return;
        }
        if !(node.is_dot() || node.is_safe_navigation()) {
            return;
        }
        check_space_before_dot(cx, node);
        check_space_after_dot(cx, node);
    }
}

fn check_space_before_dot(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let receiver_pos = node.receiver().unwrap().source_range().end_pos;
    let dot_pos = node.loc().dot.unwrap().begin_pos;
    check_space(cx, receiver_pos, dot_pos);
}

fn check_space_after_dot(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let dot_pos = node.loc().dot.unwrap().end_pos;
    let selector_pos = if node.is_method("call") && node.loc().selector.is_none() {
        node.loc().begin.unwrap().begin_pos
    } else {
        node.loc().selector.unwrap().begin_pos
    };
    check_space(cx, dot_pos, selector_pos);
}

fn check_space_after_double_colon(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let double_colon_pos = node.loc().double_colon.unwrap().end_pos;
    let name_pos = node.loc().name.unwrap().begin_pos;
    check_space(cx, double_colon_pos, name_pos);
}

fn check_space(cx: &mut Investigation<'_, '_>, begin_pos: usize, end_pos: usize) {
    if end_pos <= begin_pos {
        return;
    }
    let range = range_between(begin_pos, end_pos);
    if !cx.source(range).bytes().all(|b| b == b' ' || b == b'\t') {
        return;
    }
    cx.add_offense_with_correction(range, MSG, |corrector| corrector.remove(range));
}
