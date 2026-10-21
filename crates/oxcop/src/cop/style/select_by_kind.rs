//! Port of `lib/rubocop/cop/style/select_by_kind.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const SELECT_METHODS: &[&str] = &["select", "filter", "find_all"];

// `%CLASS_CHECK_METHODS` is inlined as `{:is_a? :kind_of?}`.
static CLASS_CHECK: NodePattern = NodePattern::new(
    "{
       (block call (args (arg $_)) ${(send (lvar _) {:is_a? :kind_of?} _)})
       (block call (args (arg $_)) ${(send (send (lvar _) {:is_a? :kind_of?} _) :!)})
       (numblock call $1 ${(send (lvar _) {:is_a? :kind_of?} _)})
       (numblock call $1 ${(send (send (lvar _) {:is_a? :kind_of?} _) :!)})
       (itblock call $_ ${(send (lvar _) {:is_a? :kind_of?} _)})
       (itblock call $_ ${(send (send (lvar _) {:is_a? :kind_of?} _) :!)})
     }",
);

pub(crate) static CREATES_HASH: NodePattern = NodePattern::new(
    "{
       (call (const _ :Hash) {:new :[]} ...)
       (block (call (const _ :Hash) :new ...) ...)
       (call _ { :to_h :to_hash } ...)
     }",
);

pub(crate) static ENV_CONST: NodePattern = NodePattern::new("(const {nil? cbase} :ENV)");

static CALLS_LVAR: NodePattern = NodePattern::new("(send (lvar %1) {:is_a? :kind_of?} _)");

static NEGATED_CALLS_LVAR: NodePattern =
    NodePattern::new("(send (send (lvar %1) {:is_a? :kind_of?} _) :!)");

#[derive(Default)]
pub struct SelectByKind;

impl Cop for SelectByKind {
    fn name(&self) -> &'static str {
        "Style/SelectByKind"
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
        let Some(block_node) = node.block_node() else {
            return;
        };
        if block_node.body().is_some_and(|b| b.is(NodeType::Begin)) {
            return;
        }
        if receiver_allowed(block_node.receiver()) {
            return;
        }
        let Some(class_check_send_node) = extract_send_node(block_node) else {
            return;
        };
        let replacement = replacement(class_check_send_node, node);
        let class_constant = find_class_constant(class_check_send_node);
        let message = format!(
            "Prefer `{replacement}` to `{}` with a kind check.",
            node.method_name().unwrap()
        );
        cx.add_offense_with_correction(block_node.source_range(), &message, |corrector| {
            if let Some(class_constant) = class_constant {
                let range = range_between(
                    node.loc().selector.unwrap().begin_pos,
                    block_node.loc().end.unwrap().end_pos,
                );
                corrector.replace(range, format!("{replacement}({})", class_constant.source()));
            }
        });
    }
}

pub(crate) fn receiver_allowed(node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    node.is(NodeType::Hash) || CREATES_HASH.is_match(node) || ENV_CONST.is_match(node)
}

/// The block argument name a matcher captured (`:"_1"` for numblocks,
/// `:it` for itblocks).
pub(crate) fn block_arg_name(block_node: Node<'_>, captured: Option<&Child>) -> String {
    match block_node.ty() {
        NodeType::Numblock => match captured {
            Some(Child::Int(i)) => format!("_{i}"),
            _ => "_1".to_owned(),
        },
        NodeType::Itblock => "it".to_owned(),
        _ => captured
            .and_then(Child::as_sym)
            .unwrap_or_default()
            .to_owned(),
    }
}

fn extract_send_node(block_node: Node<'_>) -> Option<Node<'_>> {
    let captures = CLASS_CHECK.match_node(block_node)?;
    let captured = match captures[0].val() {
        Some(crate::ast::node_pattern::Val::Value(child)) => Some(child),
        _ => None,
    };
    let name = block_arg_name(block_node, captured);
    let class_check_send_node = captures[1].node()?;
    let inner_node = unwrap_negation(class_check_send_node);
    let params = [Param::Sym(&name)];
    let ctx = MatchContext {
        params: &params,
        ..MatchContext::default()
    };
    (inner_node.is_some_and(|inner| CALLS_LVAR.match_with(inner, &ctx).is_some())
        || NEGATED_CALLS_LVAR
            .match_with(class_check_send_node, &ctx)
            .is_some())
    .then_some(class_check_send_node)
}

fn replacement(class_check_send_node: Node<'_>, node: Node<'_>) -> &'static str {
    let negated = negated(class_check_send_node);
    if SELECT_METHODS.contains(&node.method_name().unwrap_or_default()) {
        if negated { "grep_v" } else { "grep" }
    } else if negated {
        "grep"
    } else {
        "grep_v"
    }
}

fn negated(class_check_send_node: Node<'_>) -> bool {
    class_check_send_node.is_send() && class_check_send_node.is_method("!")
}

fn unwrap_negation(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_send() && node.is_method("!") {
        node.receiver()
    } else {
        Some(node)
    }
}

fn find_class_constant(node: Node<'_>) -> Option<Node<'_>> {
    let inner_node = unwrap_negation(node)?;
    if inner_node.is_send() {
        inner_node.first_argument()
    } else {
        None
    }
}
