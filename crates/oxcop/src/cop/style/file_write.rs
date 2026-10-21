//! Port of `lib/rubocop/cop/style/file_write.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const TRUNCATING_WRITE_MODES: &[&str] = &["w", "wt", "wb", "w+", "w+t", "w+b"];

static FILE_OPEN: NodePattern = NodePattern::new(
    "(send
      (const {nil? cbase} :File)
      :open
      $_
      (str $%TRUNCATING_WRITE_MODES)
      (block-pass (sym :write))?
    )",
);
static SEND_WRITE: NodePattern = NodePattern::new("(send _ :write $_)");
static BLOCK_WRITE: NodePattern =
    NodePattern::new("(block _ (args (arg $_)) (send (lvar $_) :write $_))");

#[derive(Default)]
pub struct FileWrite;

impl Cop for FileWrite {
    fn name(&self) -> &'static str {
        "Style/FileWrite"
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
            named: &[("TRUNCATING_WRITE_MODES", Param::Strs(TRUNCATING_WRITE_MODES))],
            ..Default::default()
        };
        let Some(captures) = FILE_OPEN.match_with(node, &ctx) else {
            return;
        };
        let filename = captures[0].node().unwrap();
        let mode = captures[1].str().unwrap_or("");
        let Some(write_node) = node.parent() else {
            return;
        };
        let Some(content) = file_open_write(write_node) else {
            return;
        };
        let method = write_method(mode);
        let message = format!("Use `File.{method}`.");
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(write_node.source_range(), &message, |corrector| {
            let range = range_between(
                node.loc().selector.unwrap().begin_pos,
                write_node.source_range().end_pos,
            );
            corrector.replace(range, replacement(src, mode, filename, content, write_node));
        });
    }
}

fn file_open_write(node: Node<'_>) -> Option<Node<'_>> {
    let content = if let Some(c) = SEND_WRITE.match_node(node) {
        c[0].node()
    } else if let Some(c) = BLOCK_WRITE.match_node(node) {
        if c[0].val().zip(c[1].val()).is_some_and(|(a, b)| a.sym() == b.sym()) {
            c[2].node()
        } else {
            None
        }
    } else {
        None
    }?;
    if content.is(NodeType::Splat) {
        return None;
    }
    Some(content)
}

fn write_method(mode: &str) -> &'static str {
    if mode.ends_with('b') { "binwrite" } else { "write" }
}

fn replacement(
    src: &str,
    mode: &str,
    filename: Node<'_>,
    content: Node<'_>,
    write_node: Node<'_>,
) -> String {
    let mut replacement = format!(
        "{}({}, {})",
        write_method(mode),
        filename.source(),
        content.source()
    );
    let heredocs = removed_heredocs(filename, content, write_node);
    for heredoc in heredocs {
        replacement.push('\n');
        let range = heredoc_range(heredoc);
        replacement.push_str(&src[range.begin_pos..range.end_pos]);
    }
    replacement
}

fn removed_heredocs<'a>(
    filename: Node<'a>,
    content: Node<'a>,
    write_node: Node<'_>,
) -> Vec<Node<'a>> {
    let mut heredocs: Vec<Node<'a>> = [filename, content]
        .into_iter()
        .flat_map(find_heredocs)
        .filter(|h| removed(*h, write_node))
        .collect();
    heredocs.sort_by_key(|h| h.loc().heredoc_body.unwrap().begin_pos);
    heredocs
}

fn heredoc_range(heredoc: Node<'_>) -> Range {
    range_between(
        heredoc.loc().heredoc_body.unwrap().begin_pos,
        heredoc.loc().heredoc_end.unwrap().end_pos,
    )
}

fn find_heredocs(node: Node<'_>) -> Vec<Node<'_>> {
    std::iter::once(node)
        .chain(node.descendants().filter(|d| d.is_any_str()))
        .filter(|child| child.is_any_str() && child.is_heredoc())
        .collect()
}

fn removed(heredoc: Node<'_>, write_node: Node<'_>) -> bool {
    heredoc.loc().heredoc_end.unwrap().end_pos <= write_node.source_range().end_pos
}
