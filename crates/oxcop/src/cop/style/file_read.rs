//! Port of `lib/rubocop/cop/style/file_read.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const READ_FILE_START_TO_FINISH_MODES: &[&str] = &["r", "rt", "rb", "r+", "r+t", "r+b"];

static FILE_OPEN: NodePattern = NodePattern::new(
    "(send
      (const {nil? cbase} :File)
      :open
      $_
      (str $%READ_FILE_START_TO_FINISH_MODES)?
      $(block-pass (sym :read))?
    )",
);
static SEND_READ: NodePattern = NodePattern::new("(send _ :read)");
static BLOCK_READ: NodePattern =
    NodePattern::new("(block _ (args (arg _name)) (send (lvar _name) :read))");

#[derive(Default)]
pub struct FileRead;

impl Cop for FileRead {
    fn name(&self) -> &'static str {
        "Style/FileRead"
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
            named: &[(
                "READ_FILE_START_TO_FINISH_MODES",
                Param::Strs(READ_FILE_START_TO_FINISH_MODES),
            )],
            ..Default::default()
        };
        let Some(captures) = FILE_OPEN.match_with(node, &ctx) else {
            return;
        };
        let filename = captures[0].node().unwrap();
        let mode = captures[1].list().first().and_then(|c| c.str()).unwrap_or("r");
        let read_node = if !captures[2].list().is_empty() {
            node
        } else {
            match node.parent() {
                Some(parent) if file_open_read(parent) => parent,
                _ => return,
            }
        };
        let method = read_method(mode);
        let message = format!("Use `File.{method}`.");
        cx.add_offense_with_correction(read_node.source_range(), &message, |corrector| {
            let range = range_between(
                node.loc().selector.unwrap().begin_pos,
                read_node.source_range().end_pos,
            );
            corrector.replace(range, format!("{method}({})", filename.source()));
        });
    }
}

fn file_open_read(node: Node<'_>) -> bool {
    SEND_READ.is_match(node) || BLOCK_READ.is_match(node)
}

fn read_method(mode: &str) -> &'static str {
    if mode.ends_with('b') { "binread" } else { "read" }
}
