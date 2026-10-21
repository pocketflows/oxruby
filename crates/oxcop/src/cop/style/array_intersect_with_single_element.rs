//! Port of `lib/rubocop/cop/style/array_intersect_with_single_element.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `include?(element)` instead of `intersect?([element])`.";

static SINGLE_ELEMENT: NodePattern = NodePattern::new("(send _ _ $(array $_))");

#[derive(Default)]
pub struct ArrayIntersectWithSingleElement;

impl Cop for ArrayIntersectWithSingleElement {
    fn name(&self) -> &'static str {
        "Style/ArrayIntersectWithSingleElement"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["intersect?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = SINGLE_ELEMENT.match_node(node) else {
            return;
        };
        let array = captures[0].node().unwrap();
        let Some(element) = captures[1].node() else {
            return;
        };
        if element.is(NodeType::Splat) {
            return;
        }
        let selector = node.loc().selector.unwrap();
        let range = node.source_range().with_begin(selector.begin_pos);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(selector, "include?");
            let replacement = if array.is_percent_literal() {
                inspect_value(element)
            } else {
                element.source().to_owned()
            };
            corrector.replace(array.source_range(), replacement);
        });
    }
}

/// `element.value.inspect` for the elements of `%w` and `%i` arrays.
fn inspect_value(element: Node<'_>) -> String {
    match element.ty() {
        NodeType::Sym => format!(":{}", element.sym_value().unwrap_or("")),
        _ => ruby_inspect(element.str_content().unwrap_or(element.source())),
    }
}
