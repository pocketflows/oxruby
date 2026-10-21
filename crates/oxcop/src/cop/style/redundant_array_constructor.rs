//! Port of `lib/rubocop/cop/style/redundant_array_constructor.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Remove the redundant `Array` constructor.";

#[derive(Default)]
pub struct RedundantArrayConstructor;

impl Cop for RedundantArrayConstructor {
    fn name(&self) -> &'static str {
        "Style/RedundantArrayConstructor"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new", "[]", "Array"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let array_const = node.receiver().is_some_and(|r| {
            r.is(NodeType::Const)
                && r.child_sym(1) == Some("Array")
                && r.child(0).is_none_or(|ns| ns.is(NodeType::Cbase))
        });
        let args = node.arguments();
        let single_array = args.len() == 1 && args[0].is(NodeType::Array);
        let selector = node.loc().selector.unwrap();
        let (range, replacement): (Range, Range) = match node.method_name() {
            Some("new") if array_const && single_array => (
                node.receiver().unwrap().source_range().join(selector),
                args[0].source_range(),
            ),
            Some("[]") if array_const => (
                node.receiver().unwrap().source_range(),
                selector.begin().join(node.source_range().end()),
            ),
            Some("Array") if node.receiver().is_none() && single_array => {
                (selector, args[0].source_range())
            }
            _ => return,
        };
        let replacement = node.range_source(replacement);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(node.source_range(), replacement);
        });
    }
}
