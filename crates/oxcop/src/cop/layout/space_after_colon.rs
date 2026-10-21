//! Port of `lib/rubocop/cop/layout/space_after_colon.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Space missing after colon.";

#[derive(Default)]
pub struct SpaceAfterColon;

impl Cop for SpaceAfterColon {
    fn name(&self) -> &'static str {
        "Layout/SpaceAfterColon"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Pair, NodeType::Kwoptarg]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let colon = if node.is(NodeType::Pair) {
            if !node.is_colon() || node.is_value_omission() {
                return;
            }
            node.loc().operator.unwrap()
        } else {
            node.loc().name.unwrap().end().resize(1)
        };
        if !followed_by_space(cx, colon) {
            register_offense(cx, colon);
        }
    }
}

fn register_offense(cx: &mut Investigation<'_, '_>, colon: Range) {
    cx.add_offense_with_correction(colon, MSG, |corrector| {
        corrector.insert_after(colon, " ");
    });
}

fn followed_by_space(cx: &Investigation<'_, '_>, colon: Range) -> bool {
    cx.processed_source
        .buffer_source()
        .as_bytes()
        .get(colon.end_pos)
        .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c'))
}
