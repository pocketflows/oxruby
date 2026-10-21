//! Port of `lib/rubocop/cop/lint/redundant_type_conversion.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

/// `LITERAL_NODE_TYPES`
fn literal_node_types(method: &str) -> &'static [NodeType] {
    match method {
        "to_s" => &[NodeType::Str, NodeType::Dstr],
        "to_sym" => &[NodeType::Sym, NodeType::Dsym],
        "to_i" => &[NodeType::Int],
        "to_f" => &[NodeType::Float],
        "to_r" => &[NodeType::Rational],
        "to_c" => &[NodeType::Complex],
        "to_a" => &[NodeType::Array],
        "to_h" => &[NodeType::Hash],
        _ => &[],
    }
}

/// `TYPED_METHODS`
fn typed_methods(method: &str) -> &'static [&'static str] {
    match method {
        "to_s" => &["inspect", "to_json"],
        _ => &[],
    }
}

static TYPE_CONSTRUCTOR: NodePattern =
    NodePattern::new("(send {nil? (const {cbase nil?} :Kernel)} %1 ...)");
static STRING_CONSTRUCTOR: NodePattern = NodePattern::new(
    "{
       (send (const {cbase nil?} :String) :new ...)
       #type_constructor?(:String)
     }",
);
static INTEGER_CONSTRUCTOR: NodePattern = NodePattern::new("#type_constructor?(:Integer)");
static FLOAT_CONSTRUCTOR: NodePattern = NodePattern::new("#type_constructor?(:Float)");
static BIGDECIMAL_CONSTRUCTOR: NodePattern = NodePattern::new("#type_constructor?(:BigDecimal)");
static RATIONAL_CONSTRUCTOR: NodePattern = NodePattern::new("#type_constructor?(:Rational)");
static COMPLEX_CONSTRUCTOR: NodePattern = NodePattern::new("#type_constructor?(:Complex)");
static ARRAY_CONSTRUCTOR: NodePattern = NodePattern::new(
    "{
       (send (const {cbase nil?} :Array) {:new :[]} ...)
       #type_constructor?(:Array)
     }",
);
static HASH_CONSTRUCTOR: NodePattern = NodePattern::new(
    "{
       (block (send (const {cbase nil?} :Hash) :new) ...)
       (send (const {cbase nil?} :Hash) {:new :[]} ...)
       (send {nil? (const {cbase nil?} :Kernel)} :Hash ...)
     }",
);
static SET_CONSTRUCTOR: NodePattern = NodePattern::new(
    "{
       (send (const {cbase nil?} :Set) {:new :[]} ...)
     }",
);
static EXCEPTION_FALSE_KEYWORD_ARGUMENT: NodePattern =
    NodePattern::new("(hash (pair (sym :exception) false))");

/// `CONSTRUCTOR_MAPPING`
fn constructor_matcher(method: &str) -> Option<&'static NodePattern> {
    Some(match method {
        "to_s" => &STRING_CONSTRUCTOR,
        "to_i" => &INTEGER_CONSTRUCTOR,
        "to_f" => &FLOAT_CONSTRUCTOR,
        "to_d" => &BIGDECIMAL_CONSTRUCTOR,
        "to_r" => &RATIONAL_CONSTRUCTOR,
        "to_c" => &COMPLEX_CONSTRUCTOR,
        "to_a" => &ARRAY_CONSTRUCTOR,
        "to_h" => &HASH_CONSTRUCTOR,
        "to_set" => &SET_CONSTRUCTOR,
        _ => return None,
    })
}

#[derive(Default)]
pub struct RedundantTypeConversion;

impl Cop for RedundantTypeConversion {
    fn name(&self) -> &'static str {
        "Lint/RedundantTypeConversion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "to_s", "to_sym", "to_i", "to_f", "to_r", "to_c", "to_a", "to_h", "to_set", "to_d",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_arguments() || hash_or_set_with_block(node) {
            return;
        }

        let receiver = find_receiver(node);
        if !(literal_receiver(node, receiver)
            || constructor(node, receiver)
            || chained_conversion(node, receiver)
            || chained_to_typed_method(node, receiver))
        {
            return;
        }

        let (Some(dot), Some(selector)) = (node.loc().dot, node.loc().selector) else {
            return;
        };
        let message = format!(
            "Redundant `{}` detected.",
            node.method_name().unwrap_or_default()
        );
        let end = node.loc().end.unwrap_or(selector);
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.remove(dot.join(end));
        });
    }
}

fn hash_or_set_with_block(node: Node<'_>) -> bool {
    if !node.is_method("to_h") && !node.is_method("to_set") {
        return false;
    }

    node.parent().is_some_and(|p| p.is_any_block())
        || node
            .last_argument()
            .is_some_and(|a| a.is(NodeType::BlockPass))
}

fn find_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let mut receiver = node.receiver()?;
    while receiver.is(NodeType::Begin) {
        if receiver.children().len() != 1 {
            break;
        }
        match receiver.child(0) {
            Some(child) => receiver = child,
            None => break,
        }
    }
    Some(receiver)
}

fn literal_receiver(node: Node<'_>, receiver: Option<Node<'_>>) -> bool {
    let Some(receiver) = receiver else {
        return false;
    };
    literal_node_types(node.method_name().unwrap_or_default()).contains(&receiver.ty())
}

fn constructor(node: Node<'_>, receiver: Option<Node<'_>>) -> bool {
    let Some(matcher) = constructor_matcher(node.method_name().unwrap_or_default()) else {
        return false;
    };
    let Some(receiver) = receiver else {
        return false;
    };

    let call = |name: &str, val: &Val<'_>, args: &[Val<'_>]| {
        if name != "type_constructor?" {
            return false;
        }
        let (Some(node), Some(constant)) = (val.node(), args.first().and_then(|a| a.sym())) else {
            return false;
        };
        let params = [crate::ast::node_pattern::Param::Sym(constant)];
        let ctx = MatchContext {
            params: &params,
            ..Default::default()
        };
        TYPE_CONSTRUCTOR.match_with(node, &ctx).is_some()
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    matcher.match_with(receiver, &ctx).is_some() && !constructor_suppresses_exceptions(receiver)
}

fn constructor_suppresses_exceptions(receiver: Node<'_>) -> bool {
    receiver
        .arguments()
        .into_iter()
        .any(|arg| EXCEPTION_FALSE_KEYWORD_ARGUMENT.is_match(arg))
}

fn chained_conversion(node: Node<'_>, receiver: Option<Node<'_>>) -> bool {
    let Some(receiver) = receiver.filter(|r| r.is_call()) else {
        return false;
    };
    receiver.method_name() == node.method_name()
}

fn chained_to_typed_method(node: Node<'_>, receiver: Option<Node<'_>>) -> bool {
    let Some(receiver) = receiver.filter(|r| r.is_call()) else {
        return false;
    };
    receiver
        .method_name()
        .is_some_and(|m| typed_methods(node.method_name().unwrap_or_default()).contains(&m))
}
