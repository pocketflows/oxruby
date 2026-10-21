//! Port of `lib/rubocop/cop/lint/redundant_safe_navigation.rb`.

#[path = "utils/nil_receiver_checker.rs"]
mod nil_receiver_checker;

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::{Cop, Investigation};
use nil_receiver_checker::{NIL_CLASS_METHODS, NilReceiverChecker};

const MSG: &str = "Redundant safe navigation detected, use `.` instead.";
const MSG_LITERAL: &str = "Redundant safe navigation with default literal detected.";
const MSG_NON_NIL: &str = "Redundant safe navigation on non-nil receiver (detected by analyzing previous code/method invocations).";
const GUARANTEED_INSTANCE_METHODS: &[&str] = &["to_s", "to_i", "to_f", "to_a", "to_h"];

/// `nil.methods` (the cop's `NIL_METHODS`, without the checker's `!`).
const NIL_METHODS: &[&str] = NIL_CLASS_METHODS;

static RESPOND_TO_NIL_METHOD: NodePattern =
    NodePattern::new("(csend _ :respond_to? (sym %NIL_METHODS))");
static CONVERSION_WITH_DEFAULT: NodePattern = NodePattern::new(
    "{
       (or $(csend _ :to_h) (hash))
       (or (any_block $(csend _ :to_h) ...) (hash))
       (or $(csend _ :to_a) (array))
       (or $(csend _ :to_i) (int 0))
       (or $(csend _ :to_f) (float 0.0))
       (or $(csend _ :to_s) (str empty?))
     }",
);

#[derive(Default)]
pub struct RedundantSafeNavigation;

impl Cop for RedundantSafeNavigation {
    fn name(&self) -> &'static str {
        "Lint/RedundantSafeNavigation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Csend, NodeType::Or]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Or) {
            on_or(cx, node);
            return;
        }
        let (Some(range), Some(receiver)) = (node.loc().dot, node.receiver()) else {
            return;
        };
        if cx.cop_config_bool("InferNonNilReceiver", false) {
            let additional_nil_methods = cx.cop_config_list("AdditionalNilMethods");
            let mut checker = NilReceiverChecker::new(receiver, &additional_nil_methods);
            if checker.cant_be_nil() {
                cx.add_offense_with_correction(range, MSG_NON_NIL, |corrector| {
                    corrector.replace(range, ".");
                });
                return;
            }
        }

        if guarded_by_nil_receiver(cx, node, receiver) {
            return;
        }

        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(range, ".");
        });
    }
}

fn on_or(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let Some(send_node) = CONVERSION_WITH_DEFAULT
        .match_node(node)
        .and_then(|c| c.iter().find_map(|c| c.node()))
    else {
        return;
    };
    let (Some(dot), Some(lhs)) = (send_node.loc().dot, node.lhs()) else {
        return;
    };

    let range = dot.begin().join(node.source_range().end());
    cx.add_offense_with_correction(range, MSG_LITERAL, |corrector| {
        corrector.replace(dot, ".");
        let range_with_default = lhs.source_range().end().begin().join(node.source_range().end());
        corrector.remove(range_with_default);
    });
}

fn respond_to_nil_method(node: Node<'_>) -> bool {
    let named = [("NIL_METHODS", Param::Syms(NIL_METHODS))];
    let ctx = MatchContext {
        named: &named,
        ..Default::default()
    };
    RESPOND_TO_NIL_METHOD.match_with(node, &ctx).is_some()
}

fn guarded_by_nil_receiver(cx: &Investigation<'_, '_>, node: Node<'_>, receiver: Node<'_>) -> bool {
    if assume_receiver_instance_exists(receiver) {
        return false;
    }

    let guaranteed_instance = guaranteed_instance(receiver);
    if !guaranteed_instance && !check(cx, node) {
        return true;
    }

    respond_to_nil_method(node) && !guaranteed_instance
}

fn assume_receiver_instance_exists(receiver: Node<'_>) -> bool {
    if receiver.is(NodeType::Const)
        && !crate::regex!(r"\A[[:digit:][:upper:]_]+\z")
            .find(receiver.short_name().unwrap_or_default())
            .is_some()
    {
        return true;
    }

    receiver.is(NodeType::SelfNode) || (receiver.is_literal() && !receiver.is(NodeType::Nil))
}

fn guaranteed_instance(node: Node<'_>) -> bool {
    let receiver = if node.is_any_block() {
        match node.send_node() {
            Some(send) => send,
            None => return false,
        }
    } else {
        node
    };
    if !receiver.is(NodeType::Send) {
        return false;
    }

    receiver
        .method_name()
        .is_some_and(|m| GUARANTEED_INSTANCE_METHODS.contains(&m))
}

fn check(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !allowed_method(cx, node.method_name().unwrap_or_default()) {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };

    condition(parent, node)
        || parent.is_operator_keyword()
        || (parent.is(NodeType::Send) && parent.is_negation_method())
}

fn condition(parent: Node<'_>, node: Node<'_>) -> bool {
    (parent.is_conditional() || parent.is_post_condition_loop())
        && parent.condition().is_some_and(|c| c.same_as(node))
}
