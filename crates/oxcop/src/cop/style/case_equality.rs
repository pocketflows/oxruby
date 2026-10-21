//! Port of `lib/rubocop/cop/style/case_equality.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Avoid the use of the case equality operator `===`.";

static CASE_EQUALITY: NodePattern = NodePattern::new("(send $#offending_receiver? :=== $_)");

static SELF_CLASS: NodePattern = NodePattern::new("(send (self) :class)");

#[derive(Default)]
pub struct CaseEquality;

impl Cop for CaseEquality {
    fn name(&self) -> &'static str {
        "Style/CaseEquality"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let allow_on_constant = cx.cop_config_bool("AllowOnConstant", false);
        let allow_on_self_class = cx.cop_config_bool("AllowOnSelfClass", false);
        let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
            name == "offending_receiver?"
                && offending_receiver(val.node(), allow_on_constant, allow_on_self_class)
        };
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        let Some(captures) = CASE_EQUALITY.match_with(node, &ctx) else {
            return;
        };
        let (Some(lhs), Some(rhs)) = (captures[0].node(), captures[1].node()) else {
            return;
        };
        if lhs.is(NodeType::Regexp) || (lhs.is(NodeType::Const) && !module_name(lhs)) {
            return;
        }
        cx.add_offense_with_correction(node.loc().selector.unwrap(), MSG, |corrector| {
            if let Some(replacement) = replacement(lhs, rhs) {
                corrector.replace(node.source_range(), replacement);
            }
        });
    }
}

/// `ConstNode#module_name?`: the name has a lowercase letter.
fn module_name(node: Node<'_>) -> bool {
    node.short_name()
        .is_some_and(|name| regex!(r"[[:lower:]]").find(name).is_some())
}

fn offending_receiver(node: Option<Node<'_>>, allow_constant: bool, allow_self_class: bool) -> bool {
    if allow_constant && node.is_some_and(|n| n.is(NodeType::Const)) {
        return false;
    }
    if allow_self_class && node.is_some_and(|n| SELF_CLASS.is_match(n)) {
        return false;
    }
    true
}

fn replacement(lhs: Node<'_>, rhs: Node<'_>) -> Option<String> {
    match lhs.ty() {
        NodeType::Begin => {
            if !lhs.child(0).is_some_and(|c| c.is_range()) {
                return None;
            }
            Some(format!("{}.include?({})", lhs.source(), rhs.source()))
        }
        NodeType::Const => Some(format!(
            "{}.is_a?({})",
            parenthesize_if_needed(rhs),
            lhs.source()
        )),
        NodeType::Send => {
            if !SELF_CLASS.is_match(lhs) {
                return None;
            }
            Some(format!(
                "{}.is_a?({})",
                parenthesize_if_needed(rhs),
                lhs.source()
            ))
        }
        _ => None,
    }
}

fn parenthesize_if_needed(node: Node<'_>) -> String {
    if requires_parentheses(node) {
        format!("({})", node.source())
    } else {
        node.source().to_owned()
    }
}

fn requires_parentheses(node: Node<'_>) -> bool {
    if matches!(node.ty(), NodeType::And | NodeType::Or | NodeType::If) || node.is_range() {
        return true;
    }
    // `assignment?` is `setter_method?` on send nodes.
    if node.is_call() {
        if node.is_setter_method() {
            return true;
        }
    } else if node.is_assignment() {
        return true;
    }
    node.is_send() && (node.is_operator_method() || node.is_unary_operation())
}
