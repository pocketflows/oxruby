//! Port of `lib/rubocop/cop/layout/empty_lines_around_begin_body.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::empty_lines_around_body::Body;
use crate::cop::{Cop, Investigation};

const KIND: &str = "`begin`";

#[derive(Default)]
pub struct EmptyLinesAroundBeginBody;

impl Cop for EmptyLinesAroundBeginBody {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundBeginBody"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Kwbegin]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let body = Body { kind: KIND, style: "no_empty_lines" };
        body.check(cx, node, None, None);
    }
}
