//! Port of `lib/rubocop/cop/lint/duplicate_set_element.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static SET_INIT_ELEMENTS: NodePattern = NodePattern::new(
    "{
      (send (const {nil? cbase} {:Set :SortedSet}) :[] $...)
      (send (const {nil? cbase} {:Set :SortedSet}) :new (array $...))
      (call (array $...) :to_set)
    }",
);

#[derive(Default)]
pub struct DuplicateSetElement;

impl Cop for DuplicateSetElement {
    fn name(&self) -> &'static str {
        "Lint/DuplicateSetElement"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]", "new", "to_set"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = SET_INIT_ELEMENTS.match_node(node) else {
            return;
        };
        let set_elements = captures[0].nodes();
        let mut seen_elements: Vec<Node<'a>> = Vec::new();
        for (index, &set_element) in set_elements.iter().enumerate() {
            if !set_element.is_literal()
                && !set_element.is(NodeType::Const)
                && !set_element.is_variable()
            {
                continue;
            }
            if seen_elements.iter().any(|seen| seen.same_as(set_element)) {
                register_offense(cx, set_element, set_elements[index - 1], node);
            } else {
                seen_elements.push(set_element);
            }
        }
    }
}

fn register_offense<'a>(
    cx: &mut Investigation<'a, '_>,
    current_element: Node<'a>,
    prev_element: Node<'a>,
    node: Node<'a>,
) {
    let receiver = node.receiver().unwrap();
    let class_name = if receiver.is(NodeType::Const) {
        receiver.const_name().unwrap()
    } else {
        "Set".to_owned()
    };
    let message = format!("Remove the duplicate element in {class_name}.");
    cx.add_offense_with_correction(current_element.source_range(), &message, |corrector| {
        let range = prev_element
            .source_range()
            .end()
            .join(current_element.source_range().end());
        corrector.remove(range);
    });
}
