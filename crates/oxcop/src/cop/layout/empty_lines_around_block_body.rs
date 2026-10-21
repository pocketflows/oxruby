//! Port of `lib/rubocop/cop/layout/empty_lines_around_block_body.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::empty_lines_around_body::Body;
use crate::cop::{Cop, Investigation};

const KIND: &str = "block";

#[derive(Default)]
pub struct EmptyLinesAroundBlockBody;

impl Cop for EmptyLinesAroundBlockBody {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundBlockBody"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let body = Body { kind: KIND, style: style(cx) };
        let first_line = node.send_node().map(|s| s.last_line());
        body.check(cx, node, node.body(), first_line);
    }
}
