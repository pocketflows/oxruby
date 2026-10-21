//! Port of `lib/rubocop/cop/style/even_odd.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static EVEN_ODD_CANDIDATE: NodePattern = NodePattern::new(
    "(send
       {(send $_ :% (int 2))
        (begin (send $_ :% (int 2)))}
       ${:== :!=}
       (int ${0 1}))",
);

#[derive(Default)]
pub struct EvenOdd;

impl Cop for EvenOdd {
    fn name(&self) -> &'static str {
        "Style/EvenOdd"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", "!="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = EVEN_ODD_CANDIDATE.match_node(node) else {
            return;
        };
        let Some(base_number) = captures[0].node() else {
            return;
        };
        let method = captures[1].sym().unwrap_or("");
        let arg = captures[2].val().and_then(|v| v.int()).unwrap_or(0);
        let replacement_method = match (arg, method == "==") {
            (0, true) | (1, false) => "even",
            _ => "odd",
        };
        let message = format!("Replace with `Integer#{replacement_method}?`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let correction = format!("{}.{replacement_method}?", receiver_source(base_number));
            corrector.replace(node.source_range(), correction);
        });
    }
}

fn receiver_source(node: Node<'_>) -> String {
    if node.is_send() && node.is_operator_method() && !node.is_method("[]") {
        format!("({})", node.source())
    } else {
        node.source().to_owned()
    }
}
