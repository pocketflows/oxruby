//! Port of `lib/rubocop/cop/style/select_by_range.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::style::select_by_kind::{block_arg_name, receiver_allowed};
use crate::cop::{Cop, Investigation};

const SELECT_METHODS: &[&str] = &["select", "filter", "find_all"];
const FIND_METHODS: &[&str] = &["find", "detect"];

static RANGE_CHECK: NodePattern = NodePattern::new(
    "{
       (block call (args (arg $_)) ${(send (lvar _) :between? _ _)})
       (block call (args (arg $_)) ${(send {range (begin range)} {:cover? :include?} (lvar _))})
       (block call (args (arg $_)) ${(send (send (lvar _) :between? _ _) :!)})
       (block call (args (arg $_)) ${(send (send {range (begin range)} {:cover? :include?} (lvar _)) :!)})
       (block call (args (arg $_)) ${(send (begin (send (lvar _) :between? _ _)) :!)})
       (block call (args (arg $_)) ${(send (begin (send {range (begin range)} {:cover? :include?} (lvar _))) :!)})
       (numblock call $1 ${(send (lvar _) :between? _ _)})
       (numblock call $1 ${(send {range (begin range)} {:cover? :include?} (lvar _))})
       (numblock call $1 ${(send (send (lvar _) :between? _ _) :!)})
       (numblock call $1 ${(send (send {range (begin range)} {:cover? :include?} (lvar _)) :!)})
       (numblock call $1 ${(send (begin (send (lvar _) :between? _ _)) :!)})
       (numblock call $1 ${(send (begin (send {range (begin range)} {:cover? :include?} (lvar _))) :!)})
       (itblock call $_ ${(send (lvar _) :between? _ _)})
       (itblock call $_ ${(send {range (begin range)} {:cover? :include?} (lvar _))})
       (itblock call $_ ${(send (send (lvar _) :between? _ _) :!)})
       (itblock call $_ ${(send (send {range (begin range)} {:cover? :include?} (lvar _)) :!)})
       (itblock call $_ ${(send (begin (send (lvar _) :between? _ _)) :!)})
       (itblock call $_ ${(send (begin (send {range (begin range)} {:cover? :include?} (lvar _))) :!)})
     }",
);

static BETWEEN_CALL: NodePattern = NodePattern::new("(send (lvar %1) :between? _ _)");

static RANGE_COVER_CALL: NodePattern =
    NodePattern::new("(send {range (begin range)} {:cover? :include?} (lvar %1))");

#[derive(Default)]
pub struct SelectByRange;

impl Cop for SelectByRange {
    fn name(&self) -> &'static str {
        "Style/SelectByRange"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["select", "filter", "find_all", "reject", "find", "detect"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(block_node) = node.block_node() else {
            return;
        };
        if block_node.body().is_some_and(|b| b.is(NodeType::Begin)) {
            return;
        }
        if receiver_allowed(block_node.receiver()) {
            return;
        }
        let Some(range_check_send_node) = extract_send_node(block_node) else {
            return;
        };
        let replacement = replacement(range_check_send_node, node);
        let range_literal = find_range(range_check_send_node);
        let message = format!(
            "Prefer `{replacement}` to `{}` with a range check.",
            node.method_name().unwrap()
        );
        cx.add_offense_with_correction(block_node.source_range(), &message, |corrector| {
            let range = range_between(
                node.loc().selector.unwrap().begin_pos,
                block_node.loc().end.unwrap().end_pos,
            );
            let grep_method = if replacement.contains("grep_v") {
                "grep_v"
            } else {
                "grep"
            };
            let suffix = if replacement.contains(".first") {
                ".first"
            } else {
                ""
            };
            corrector.replace(range, format!("{grep_method}({range_literal}){suffix}"));
        });
    }
}

fn replacement(range_check_send_node: Node<'_>, node: Node<'_>) -> &'static str {
    let negated = range_check_send_node.is_send() && range_check_send_node.is_method("!");
    let method_name = node.method_name().unwrap_or_default();
    if SELECT_METHODS.contains(&method_name) {
        if negated { "grep_v" } else { "grep" }
    } else if FIND_METHODS.contains(&method_name) {
        if negated {
            "grep_v(...).first"
        } else {
            "grep(...).first"
        }
    } else if negated {
        "grep"
    } else {
        "grep_v"
    }
}

fn extract_send_node(block_node: Node<'_>) -> Option<Node<'_>> {
    let captures = RANGE_CHECK.match_node(block_node)?;
    let captured = match captures[0].val() {
        Some(Val::Value(child)) => Some(child),
        _ => None,
    };
    let name = block_arg_name(block_node, captured);
    let range_check_send_node = captures[1].node()?;
    let inner_node = unwrap_negation(range_check_send_node);
    let params = [Param::Sym(&name)];
    let ctx = MatchContext {
        params: &params,
        ..MatchContext::default()
    };
    (BETWEEN_CALL.match_with(inner_node, &ctx).is_some()
        || RANGE_COVER_CALL.match_with(inner_node, &ctx).is_some())
    .then_some(range_check_send_node)
}

fn unwrap_negation(node: Node<'_>) -> Node<'_> {
    if node.is_send() && node.is_method("!") {
        let receiver = node.receiver().unwrap();
        if receiver.is(NodeType::Begin) {
            receiver.child(0).unwrap()
        } else {
            receiver
        }
    } else {
        node
    }
}

fn find_range(node: Node<'_>) -> String {
    let inner = unwrap_negation(node);
    if inner.is_method("between?") {
        let args = inner.arguments();
        format!("{}..{}", args[0].source(), args[1].source())
    } else {
        let mut receiver = inner.receiver().unwrap();
        if receiver.is(NodeType::Begin) {
            receiver = receiver.child(0).unwrap();
        }
        receiver.source().to_owned()
    }
}
