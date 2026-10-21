//! Port of `lib/rubocop/cop/style/numeric_predicate.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

static PREDICATE: NodePattern =
    NodePattern::new("(send $(...) ${:zero? :positive? :negative?})");
static COMPARISON: NodePattern =
    NodePattern::new("(send [$(...) !gvar_type?] ${:== :> :<} (int 0))");
static INVERTED_COMPARISON: NodePattern =
    NodePattern::new("(send (int 0) ${:== :> :<} [$(...) !gvar_type?])");

/// `REPLACEMENTS`: predicate → operator.
fn operator_for(predicate: &str) -> &'static str {
    match predicate {
        "zero?" => "==",
        "positive?" => ">",
        "negative?" => "<",
        _ => "",
    }
}

/// `REPLACEMENTS.invert`.
fn predicate_for(operator: &str) -> &'static str {
    match operator {
        "==" => "zero?",
        ">" => "positive?",
        "<" => "negative?",
        _ => "",
    }
}

#[derive(Default)]
pub struct NumericPredicate;

impl Cop for NumericPredicate {
    fn name(&self) -> &'static str {
        "Style/NumericPredicate"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", ">", "<", "positive?", "negative?", "zero?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(replacement) = check(cx, node) else {
            return;
        };
        if allowed_method_name(cx, node.method_name().unwrap_or(""))
            || node.ancestors().any(|ancestor| {
                (ancestor.is(NodeType::Send) || ancestor.is_any_block())
                    && allowed_method_name(cx, ancestor.method_name().unwrap_or(""))
            })
        {
            return;
        }
        let message = format!("Use `{replacement}` instead of `{}`.", node.source());
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), &replacement);
        });
    }
}

fn allowed_method_name(cx: &Investigation<'_, '_>, name: &str) -> bool {
    allowed_method(cx, name) || matches_allowed_pattern(cx, name)
}

fn check(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<String> {
    let predicate_style = style(cx) == "predicate";
    let (numeric, operator) = if predicate_style {
        if let Some(c) = COMPARISON.match_node(node) {
            (c[0].node()?, c[1].sym()?.to_owned())
        } else {
            let c = INVERTED_COMPARISON.match_node(node)?;
            let comparison = match c[0].sym()? {
                ">" => "<",
                "<" => ">",
                other => other,
            };
            (c[1].node()?, comparison.to_owned())
        }
    } else {
        let c = PREDICATE.match_node(node)?;
        (c[0].node()?, c[1].sym()?.to_owned())
    };
    Some(replacement(node, numeric, &operator, predicate_style))
}

fn replacement(node: Node<'_>, numeric: Node<'_>, operation: &str, predicate_style: bool) -> String {
    if predicate_style {
        format!("{}.{}", parenthesized_source(numeric), predicate_for(operation))
    } else if negated(node) {
        format!("({} {} 0)", numeric.source(), operator_for(operation))
    } else {
        format!("{} {} 0", numeric.source(), operator_for(operation))
    }
}

fn parenthesized_source(node: Node<'_>) -> String {
    if node.is_send() && node.is_binary_operation() && !node.is_parenthesized() {
        format!("({})", node.source())
    } else {
        node.source().to_owned()
    }
}

fn negated(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is_send() && p.is_method("!"))
}
