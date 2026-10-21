//! Port of `lib/rubocop/cop/style/dir.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `__dir__` to get an absolute path to the current file's directory.";

static DIR_REPLACEMENT: NodePattern = NodePattern::new(
    "{(send (const {nil? cbase} :File) :expand_path (send (const {nil? cbase} :File) :dirname  #file_keyword?))
      (send (const {nil? cbase} :File) :dirname     (send (const {nil? cbase} :File) :realpath #file_keyword?))}",
);

#[derive(Default)]
pub struct Dir;

impl Cop for Dir {
    fn name(&self) -> &'static str {
        "Style/Dir"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["expand_path", "dirname"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
            name == "file_keyword?" && val.node().is_some_and(file_keyword)
        };
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        if DIR_REPLACEMENT.match_with(node, &ctx).is_none() {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.replace(node.source_range(), "__dir__");
        });
    }
}

fn file_keyword(node: Node<'_>) -> bool {
    node.is(NodeType::Str) && node.source() == "__FILE__"
}
