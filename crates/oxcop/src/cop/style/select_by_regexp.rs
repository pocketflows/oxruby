//! Port of `lib/rubocop/cop/style/select_by_regexp.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::style::select_by_kind::{block_arg_name, receiver_allowed};
use crate::cop::{Cop, Investigation};

const SELECT_METHODS: &[&str] = &["select", "filter", "find_all"];

// `%REGEXP_METHODS` is inlined as `{:match? :=~}`, `%REGEXP_METHODS_NEGATED`
// as `:!~`.
static REGEXP_MATCH: NodePattern = NodePattern::new(
    "{
       (block call (args (arg $_)) ${(send _ {:match? :=~} _) (send _ :!~ _) match_with_lvasgn})
       (block call (args (arg $_)) ${(send (send _ {:match? :=~} _) :!) (send (begin (send _ {:match? :=~} _)) :!) (send match_with_lvasgn :!) (send (begin match_with_lvasgn) :!)})
       (numblock call $1 ${(send _ {:match? :=~} _) (send _ :!~ _) match_with_lvasgn})
       (numblock call $1 ${(send (send _ {:match? :=~} _) :!) (send (begin (send _ {:match? :=~} _)) :!) (send match_with_lvasgn :!) (send (begin match_with_lvasgn) :!)})
       (itblock call $_ ${(send _ {:match? :=~} _) (send _ :!~ _) match_with_lvasgn})
       (itblock call $_ ${(send (send _ {:match? :=~} _) :!) (send (begin (send _ {:match? :=~} _)) :!) (send match_with_lvasgn :!) (send (begin match_with_lvasgn) :!)})
     }",
);

static CALLS_LVAR: NodePattern = NodePattern::new(
    "{
       (send (lvar %1) ...)
       (send ... (lvar %1))
       (match_with_lvasgn regexp (lvar %1))
       (send (send (lvar %1) ...) :!)
       (send (send ... (lvar %1)) :!)
       (send (match_with_lvasgn regexp (lvar %1)) :!)
       (send (begin (send (lvar %1) ...)) :!)
       (send (begin (send ... (lvar %1))) :!)
       (send (begin (match_with_lvasgn regexp (lvar %1))) :!)
     }",
);

#[derive(Default)]
pub struct SelectByRegexp;

impl Cop for SelectByRegexp {
    fn name(&self) -> &'static str {
        "Style/SelectByRegexp"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["select", "filter", "find_all", "reject"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if cx.target_ruby_version() < 2.6 && node.is_method("filter") {
            return;
        }
        let Some(block_node) = node.block_node() else {
            return;
        };
        if block_node.body().is_some_and(|b| b.is(NodeType::Begin)) {
            return;
        }
        if receiver_allowed(block_node.receiver()) {
            return;
        }
        let Some(regexp_method_send_node) = extract_send_node(block_node) else {
            return;
        };
        if match_predicate_without_receiver(regexp_method_send_node) {
            return;
        }
        let replacement = replacement(regexp_method_send_node, node);
        if cx.target_ruby_version() <= 2.2 && replacement.contains("grep_v") {
            return;
        }
        let regexp = find_regexp(regexp_method_send_node, block_node);
        let message = format!(
            "Prefer `{replacement}` to `{}` with a regexp match.",
            node.method_name().unwrap()
        );
        cx.add_offense_with_correction(block_node.source_range(), &message, |corrector| {
            if let Some(regexp) = regexp {
                let range = range_between(
                    node.loc().selector.unwrap().begin_pos,
                    block_node.loc().end.unwrap().end_pos,
                );
                corrector.replace(range, format!("{replacement}({})", regexp.source()));
            }
        });
    }
}

fn replacement(regexp_method_send_node: Node<'_>, node: Node<'_>) -> &'static str {
    let negated = negated(regexp_method_send_node);
    if SELECT_METHODS.contains(&node.method_name().unwrap_or_default()) {
        if negated { "grep_v" } else { "grep" }
    } else if negated {
        "grep"
    } else {
        "grep_v"
    }
}

fn extract_send_node(block_node: Node<'_>) -> Option<Node<'_>> {
    let captures = REGEXP_MATCH.match_node(block_node)?;
    let captured = match captures[0].val() {
        Some(Val::Value(child)) => Some(child),
        _ => None,
    };
    let name = block_arg_name(block_node, captured);
    let regexp_method_send_node = captures[1].node()?;
    let params = [Param::Sym(&name)];
    let ctx = MatchContext {
        params: &params,
        ..MatchContext::default()
    };
    CALLS_LVAR
        .match_with(regexp_method_send_node, &ctx)
        .map(|_| regexp_method_send_node)
}

fn negated(regexp_method_send_node: Node<'_>) -> bool {
    if regexp_method_send_node.is_send() && regexp_method_send_node.is_method("!") {
        return true;
    }
    let inner = unwrap_negation(regexp_method_send_node);
    inner.is_send() && inner.is_method("!~")
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

fn find_regexp<'a>(node: Node<'a>, block: Node<'a>) -> Option<Node<'a>> {
    let inner = unwrap_negation(node);
    if inner.is(NodeType::MatchWithLvasgn) {
        return inner.child_nodes().next();
    }
    let receiver = inner.receiver();
    if receiver.is_some_and(|r| r.is(NodeType::Lvar))
        && (block.is_any(&[NodeType::Numblock, NodeType::Itblock])
            || receiver.map(|r| r.source())
                == block.first_argument().map(|a| a.source()))
    {
        inner.first_argument()
    } else if inner
        .first_argument()
        .is_some_and(|a| a.is(NodeType::Lvar))
    {
        receiver
    } else {
        None
    }
}

fn match_predicate_without_receiver(node: Node<'_>) -> bool {
    let inner = unwrap_negation(node);
    inner.is_send() && inner.is_method("match?") && inner.receiver().is_none()
}
