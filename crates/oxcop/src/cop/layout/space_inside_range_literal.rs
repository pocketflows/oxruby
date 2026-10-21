//! Port of `lib/rubocop/cop/layout/space_inside_range_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Space inside range literal.";

#[derive(Default)]
pub struct SpaceInsideRangeLiteral;

impl Cop for SpaceInsideRangeLiteral {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideRangeLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Irange, NodeType::Erange]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        check(cx, node);
    }
}

fn check(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let op = node.range_source(node.loc().operator.unwrap());
    let escaped_op = op.replace('.', r"\.");
    let sub = |pattern: &str, text: &str| ruby_regex(pattern).unwrap().replace(text, op);
    let expression = sub(&format!(r"{escaped_op}\n\s*"), node.source());
    let spaced = ruby_regex(&format!(r"(\s{escaped_op})|({escaped_op}\s)")).unwrap();
    if spaced.find(&expression).is_none() {
        return;
    }
    let corrected = sub(
        &format!(r"{escaped_op}\s+"),
        &sub(&format!(r"\s+{escaped_op}"), &expression),
    );
    let range = node.source_range();
    cx.add_offense_with_correction(range, MSG, |corrector| {
        corrector.replace(range, corrected);
    });
}
