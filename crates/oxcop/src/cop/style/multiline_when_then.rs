//! Port of `lib/rubocop/cop/style/multiline_when_then.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not use `then` for multiline `when` statement.";

#[derive(Default)]
pub struct MultilineWhenThen;

impl Cop for MultilineWhenThen {
    fn name(&self) -> &'static str {
        "Style/MultilineWhenThen"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::When]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.has_then() || require_then(cx, node) {
            return;
        }
        let range = node.loc().begin.unwrap();
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.remove(range_with_surrounding_space(
                src,
                range,
                SurroundingSpace {
                    side: Side::Left,
                    newlines: false,
                    ..SurroundingSpace::default()
                },
            ));
        });
    }
}

/// Requires `then` for write `when` and its body on the same line.
fn require_then(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let conditions = node.conditions();
    let (Some(first), Some(last)) = (conditions.first(), conditions.last()) else {
        return true;
    };
    if first.first_line() != last.last_line() {
        return true;
    }
    let Some(body) = node.body() else {
        return false;
    };
    same_line(cx.processed_source, node.source_range(), body.source_range())
}
