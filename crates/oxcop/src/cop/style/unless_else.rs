//! Port of `lib/rubocop/cop/style/unless_else.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Do not use `unless` with `else`. Rewrite these with the positive case first.";

#[derive(Default)]
pub struct UnlessElse {
    ignored: IgnoredNodes,
}

impl Cop for UnlessElse {
    fn name(&self) -> &'static str {
        "Style/UnlessElse"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_unless() && node.has_else()) {
            return;
        }
        let ignored = self.ignored.part_of(node);
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            if ignored {
                return;
            }
            corrector.replace(node.loc().keyword.unwrap(), "if");
            let body_range = range_between_condition_and_else(node);
            let else_range = range_between_else_and_end(node);
            corrector.swap(src, body_range, else_range);
        });
        self.ignored.ignore(node);
    }
}

fn range_between_condition_and_else(node: Node<'_>) -> Range {
    let range = match node.loc().begin {
        Some(begin) => begin.end(),
        None => node.condition().unwrap().source_range(),
    };
    range.end().join(node.loc().else_.unwrap().begin())
}

fn range_between_else_and_end(node: Node<'_>) -> Range {
    node.loc()
        .else_
        .unwrap()
        .end()
        .join(node.loc().end.unwrap().begin())
}
