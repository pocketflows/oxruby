//! Port of `lib/rubocop/cop/style/file_empty.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static OFFENSIVE: NodePattern = NodePattern::new(
    "{
      (send $(const {nil? cbase} {:File :FileTest}) :zero? $_)
      (send (send $(const {nil? cbase} {:File :FileTest}) :size $_) {:== :>=} (int 0))
      (send (send (send $(const {nil? cbase} {:File :FileTest}) :size $_) :!) {:== :>=} (int 0))
      (send (send $(const {nil? cbase} {:File :FileTest}) :size $_) :zero?)
      (send (send $(const {nil? cbase} {:File :FileTest}) {:read :binread} $_) {:!= :==} (str empty?))
      (send (send (send $(const {nil? cbase} {:File :FileTest}) {:read :binread} $_) :!) {:!= :==} (str empty?))
      (send (send $(const {nil? cbase} {:File :FileTest}) {:read :binread} $_) :empty?)
    }",
);

#[derive(Default)]
pub struct FileEmpty;

impl Cop for FileEmpty {
    fn name(&self) -> &'static str {
        "Style/FileEmpty"
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
        &[">=", "!=", "==", "zero?", "empty?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = OFFENSIVE.match_node(node) else {
            return;
        };
        let (Some(const_node), Some(arg_node)) = (captures[0].node(), captures[1].node()) else {
            return;
        };
        let message = format!(
            "Use `{}.empty?({})` instead.",
            const_node.source(),
            arg_node.source()
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(
                node.source_range(),
                format!(
                    "{}{}.empty?({})",
                    bang(node),
                    const_node.source(),
                    arg_node.source()
                ),
            );
        });
    }
}

fn bang(node: Node<'_>) -> &'static str {
    let first_negated = node.child_nodes().next().is_some_and(|c| c.is_method("!"));
    if (node.is_method("==") && first_negated)
        || (matches!(node.method_name(), Some(">=" | "!=")) && !first_negated)
    {
        "!"
    } else {
        ""
    }
}
