//! Port of `lib/rubocop/cop/style/dir_empty.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static OFFENSIVE: NodePattern = NodePattern::new(
    "{
       (send (send (send $(const {nil? cbase} :Dir) :entries $_) :size) {:== :!= :>} (int 2))
       (send (send (send $(const {nil? cbase} :Dir) :children $_) :size) {:== :!= :>} (int 0))
       (send (send $(const {nil? cbase} :Dir) :children $_) :empty?)
       (send (send $(const {nil? cbase} :Dir) :each_child $_) :none?)
     }",
);

#[derive(Default)]
pub struct DirEmpty;

impl Cop for DirEmpty {
    fn name(&self) -> &'static str {
        "Style/DirEmpty"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.4)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", "!=", ">", "empty?", "none?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_block_literal() {
            return;
        }
        let Some(captures) = OFFENSIVE.match_node(node) else {
            return;
        };
        let (Some(const_node), Some(arg_node)) = (captures[0].node(), captures[1].node()) else {
            return;
        };
        let bang = if matches!(node.method_name(), Some("!=" | ">")) {
            "!"
        } else {
            ""
        };
        let replacement = format!(
            "{bang}{}.empty?({})",
            const_node.source(),
            arg_node.source()
        );
        let message = format!("Use `{replacement}` instead.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), replacement);
        });
    }
}
