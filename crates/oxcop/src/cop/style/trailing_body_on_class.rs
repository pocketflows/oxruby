//! Port of `lib/rubocop/cop/style/trailing_body_on_class.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::line_break_corrector;
use crate::cop::mixin::trailing_body::{first_part_of, last_child, trailing_body};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Place the first line of class body on its own line.";

#[derive(Default)]
pub struct TrailingBodyOnClass;

impl Cop for TrailingBodyOnClass {
    fn name(&self) -> &'static str {
        "Style/TrailingBodyOnClass"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Sclass]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !trailing_body(node) {
            return;
        }
        let range = first_part_of(last_child(node).unwrap());
        let width = configured_indentation_width(cx);
        let ps = cx.processed_source;
        cx.add_offense_with_correction(range, MSG, |corrector| {
            line_break_corrector::correct_trailing_body(width, corrector, node, ps);
        });
    }
}

