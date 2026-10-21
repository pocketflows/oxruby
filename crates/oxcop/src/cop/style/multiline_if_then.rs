//! Port of `lib/rubocop/cop/style/multiline_if_then.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::on_normal_if_unless::normal_if_unless;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct MultilineIfThen;

impl Cop for MultilineIfThen {
    fn name(&self) -> &'static str {
        "Style/MultilineIfThen"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !normal_if_unless(node) || !non_modifier_then(cx, node) {
            return;
        }
        let begin = node.loc().begin.unwrap();
        let message = format!("Do not use `then` for multi-line `{}`.", node.keyword());
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(begin, &message, |corrector| {
            corrector.remove(range_with_surrounding_space(
                src,
                begin,
                SurroundingSpace::side(Side::Left),
            ));
        });
    }
}

fn non_modifier_then(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is_multiline()
        && own_then(node)
        && Some(cx.processed_source.line_of(node.loc().begin.unwrap().begin_pos))
            != node.if_branch().map(|b| b.first_line())
}

/// Prism gives an `elsif` with no `then` of its own the previous branch's
/// `then` location, which lies outside the node.
fn own_then(node: Node<'_>) -> bool {
    node.has_then() && node.source_range().contains(node.loc().begin.unwrap())
}
