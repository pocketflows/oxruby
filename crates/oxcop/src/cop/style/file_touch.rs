//! Port of `lib/rubocop/cop/style/file_touch.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const APPEND_FILE_MODES: &[&str] = &["a", "a+", "ab", "a+b", "at", "a+t"];

static FILE_OPEN: NodePattern = NodePattern::new(
    "(send
      (const {nil? cbase} :File) :open
      $(...)
      (str %APPEND_FILE_MODES))",
);

#[derive(Default)]
pub struct FileTouch;

impl Cop for FileTouch {
    fn name(&self) -> &'static str {
        "Style/FileTouch"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["open"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ctx = MatchContext {
            named: &[("APPEND_FILE_MODES", Param::Strs(APPEND_FILE_MODES))],
            ..Default::default()
        };
        let Some(filename) = FILE_OPEN.match_with(node, &ctx).and_then(|c| c[0].node()) else {
            return;
        };
        let Some(parent) = node.parent().filter(|p| empty_block(*p)) else {
            return;
        };
        let message = format!(
            "Use `FileUtils.touch({})` instead of `File.open` in append mode with empty block.",
            filename.source()
        );
        cx.add_offense_with_correction(parent.source_range(), &message, |corrector| {
            corrector.replace(
                parent.source_range(),
                format!("FileUtils.touch({})", filename.source()),
            );
        });
    }
}

fn empty_block(node: Node<'_>) -> bool {
    node.is(NodeType::Block) && node.body().is_none()
}
