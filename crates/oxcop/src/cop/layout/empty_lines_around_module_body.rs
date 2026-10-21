//! Port of `lib/rubocop/cop/layout/empty_lines_around_module_body.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::empty_lines_around_body::Body;
use crate::cop::{Cop, Investigation};

const KIND: &str = "module";

#[derive(Default)]
pub struct EmptyLinesAroundModuleBody;

impl Cop for EmptyLinesAroundModuleBody {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundModuleBody"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let body = Body { kind: KIND, style: style(cx) };
        body.check(cx, node, node.body(), None);
    }
}
