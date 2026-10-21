//! Port of `lib/rubocop/cop/layout/empty_lines_around_class_body.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::empty_lines_around_body::Body;
use crate::cop::{Cop, Investigation};

const KIND: &str = "class";

#[derive(Default)]
pub struct EmptyLinesAroundClassBody;

impl Cop for EmptyLinesAroundClassBody {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundClassBody"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let body = Body { kind: KIND, style: style(cx) };
        if node.is(NodeType::Class) {
            let first_line = node.parent_class().map(|p| p.last_line());
            body.check(cx, node, node.body(), first_line);
        } else {
            body.check(cx, node, node.body(), None);
        }
    }
}
