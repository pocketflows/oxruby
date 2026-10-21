//! Port of `lib/rubocop/cop/mixin/negative_conditional.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Corrector, Investigation};

pub const MSG: &str = "Favor `%<inverse>s` over `%<current>s` for negative conditions.";

/// `single_negative?`: `(send !(send _ :!) :!)`.
pub fn single_negative(node: Node<'_>) -> bool {
    node.is_send()
        && node.method_name() == Some("!")
        && node.children().len() == 2
        && !node
            .receiver()
            .is_some_and(|r| r.is_send() && r.method_name() == Some("!") && r.children().len() == 2)
}

/// `check_negative_conditional(node, message:)`.
pub fn check_negative_conditional<'a>(
    cx: &mut Investigation<'a, '_>,
    node: Node<'a>,
    message: &str,
    correct: impl FnOnce(&mut Corrector),
) {
    let Some(mut condition) = node.condition() else {
        return;
    };
    if condition.is(NodeType::Begin) && condition.children().is_empty() {
        return;
    }
    while condition.is(NodeType::Begin) {
        match condition.child_from_end(1) {
            Some(last) => condition = last,
            None => return,
        }
    }
    if !single_negative(condition) {
        return;
    }
    if node.is(NodeType::If) && node.has_else() {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), message, correct);
}

/// Port of `ConditionCorrector.correct_negative_condition`.
pub fn correct_negative_condition(corrector: &mut Corrector, node: Node<'_>) {
    let mut condition = node.condition().unwrap();
    while condition.is(NodeType::Begin) {
        condition = condition.child_from_end(1).unwrap();
    }
    corrector.replace(node.loc().keyword.unwrap(), node.inverse_keyword());
    corrector.replace(
        condition.source_range(),
        condition.child(0).map_or("", |c| c.source()),
    );
}
