//! Port of `lib/rubocop/cop/style/float_division.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Corrector, Investigation};

static RIGHT_COERCE: NodePattern = NodePattern::new("(send _ :/ #to_f_method?)");
static LEFT_COERCE: NodePattern = NodePattern::new("(send #to_f_method? :/ _)");
static BOTH_COERCE: NodePattern = NodePattern::new("(send #to_f_method? :/ #to_f_method?)");
static ANY_COERCE: NodePattern =
    NodePattern::new("{(send _ :/ #to_f_method?) (send #to_f_method? :/ _)}");
static TO_F_METHOD: NodePattern = NodePattern::new("(send !nil? :to_f)");
static REGEXP_LAST_MATCH: NodePattern = NodePattern::new(
    "{
      (send (const {nil? cbase} :Regexp) :last_match int)
      (:nth_ref _)
    }",
);

fn to_f_method(node: Node<'_>) -> bool {
    TO_F_METHOD.is_match(node)
}

fn matches(pattern: &NodePattern, node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "to_f_method?" && val.node().is_some_and(to_f_method)
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    pattern.match_with(node, &ctx).is_some()
}

#[derive(Default)]
pub struct FloatDivision;

impl Cop for FloatDivision {
    fn name(&self) -> &'static str {
        "Style/FloatDivision"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["/"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        if !offense_condition(style, node) {
            return;
        }
        let message = match style {
            "left_coerce" => "Prefer using `.to_f` on the left side.",
            "right_coerce" => "Prefer using `.to_f` on the right side.",
            "single_coerce" => "Prefer using `.to_f` on one side only.",
            _ => "Prefer using `fdiv` for float divisions.",
        };
        let (Some(receiver), Some(argument)) = (node.receiver(), node.first_argument()) else {
            return;
        };
        cx.add_offense_with_correction(node.source_range(), message, |corrector| match style {
            "left_coerce" | "single_coerce" => {
                add_to_f_method(corrector, receiver);
                remove_to_f_method(corrector, argument);
            }
            "right_coerce" => {
                remove_to_f_method(corrector, receiver);
                add_to_f_method(corrector, argument);
            }
            "fdiv" => correct_from_slash_to_fdiv(corrector, receiver, argument),
            _ => {}
        });
    }
}

fn regexp_last_match(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| REGEXP_LAST_MATCH.is_match(n))
}

fn offense_condition(style: &str, node: Node<'_>) -> bool {
    if regexp_last_match(node.receiver().and_then(|r| r.receiver()))
        || regexp_last_match(node.first_argument().and_then(|a| a.receiver()))
    {
        return false;
    }
    match style {
        "left_coerce" => matches(&RIGHT_COERCE, node),
        "right_coerce" => matches(&LEFT_COERCE, node),
        "single_coerce" => matches(&BOTH_COERCE, node),
        "fdiv" => matches(&ANY_COERCE, node),
        _ => false,
    }
}

fn add_to_f_method(corrector: &mut Corrector, node: Node<'_>) {
    if !(node.is(NodeType::Send) && node.is_method("to_f")) {
        corrector.insert_after(node.source_range(), ".to_f");
    }
}

fn remove_to_f_method(corrector: &mut Corrector, send_node: Node<'_>) {
    if let Some(dot) = send_node.loc().dot {
        corrector.remove(dot);
    }
    if let Some(selector) = send_node.loc().selector {
        corrector.remove(selector);
    }
}

fn correct_from_slash_to_fdiv(corrector: &mut Corrector, receiver: Node<'_>, argument: Node<'_>) {
    if to_f_method(receiver) {
        remove_to_f_method(corrector, receiver);
    }
    let argument_parenthesized = argument.is(NodeType::Begin);
    corrector.replace(
        receiver
            .source_range()
            .end()
            .join(argument.source_range().begin()),
        if argument_parenthesized {
            ".fdiv"
        } else {
            ".fdiv("
        },
    );
    if to_f_method(argument) {
        remove_to_f_method(corrector, argument);
    }
    if !argument_parenthesized {
        corrector.insert_after(argument.source_range(), ")");
    }
}
