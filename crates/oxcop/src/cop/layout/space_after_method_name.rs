//! Port of `lib/rubocop/cop/layout/space_after_method_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not put a space between a method name and the opening parenthesis.";

#[derive(Default)]
pub struct SpaceAfterMethodName;

impl Cop for SpaceAfterMethodName {
    fn name(&self) -> &'static str {
        "Layout/SpaceAfterMethodName"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(args) = node.arguments_node() else {
            return;
        };
        if !args.loc_is(args.loc().begin, "(") {
            return;
        }
        let expr = args.source_range();
        if expr.begin_pos == 0 {
            return;
        }
        let pos_before_left_paren = range_between(expr.begin_pos - 1, expr.begin_pos);
        if !cx.source(pos_before_left_paren).starts_with(' ') {
            return;
        }
        cx.add_offense_with_correction(pos_before_left_paren, MSG, |corrector| {
            corrector.remove(pos_before_left_paren);
        });
    }
}
