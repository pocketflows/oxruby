//! Port of `lib/rubocop/cop/style/redundant_exception.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG_1: &str = "Redundant `RuntimeError` argument can be removed.";
const MSG_2: &str = "Redundant `RuntimeError.new` call can be replaced with just the message.";

static EXPLODED: NodePattern =
    NodePattern::new("(send nil? ${:raise :fail} (const {nil? cbase} :RuntimeError) $_)");
static COMPACT: NodePattern = NodePattern::new(
    "(send nil? {:raise :fail} $(send (const {nil? cbase} :RuntimeError) :new $_))",
);

#[derive(Default)]
pub struct RedundantException;

impl Cop for RedundantException {
    fn name(&self) -> &'static str {
        "Style/RedundantException"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["raise", "fail"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(c) = EXPLODED.match_node(node) {
            let command = c[0].sym().unwrap_or("");
            let message = c[1].node().unwrap();
            if message.is(NodeType::Nil) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), MSG_1, |corrector| {
                let arg = if message.is_any_str() {
                    message.source().to_owned()
                } else {
                    format!("{}.to_s", message.source())
                };
                let arg = if node.is_parenthesized() {
                    format!("({arg})")
                } else {
                    format!(" {arg}")
                };
                corrector.replace(node.source_range(), format!("{command}{arg}"));
            });
        } else if let Some(c) = COMPACT.match_node(node) {
            let new_call = c[0].node().unwrap();
            let message = c[1].node().unwrap();
            if message.is(NodeType::Nil) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), MSG_2, |corrector| {
                let replacement = if message.is_any_str() {
                    message.source().to_owned()
                } else {
                    format!("{}.to_s", message.source())
                };
                corrector.replace(new_call.source_range(), replacement);
            });
        }
    }
}
