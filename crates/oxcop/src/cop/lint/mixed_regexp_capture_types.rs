//! Port of `lib/rubocop/cop/lint/mixed_regexp_capture_types.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::{each_capture, parsed_tree};

const MSG: &str = "Do not mix named captures and numbered captures in a Regexp literal.";

#[derive(Default)]
pub struct MixedRegexpCaptureTypes;

impl Cop for MixedRegexpCaptureTypes {
    fn name(&self) -> &'static str {
        "Lint/MixedRegexpCaptureTypes"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // `interpolation?`
        if node.child_nodes().any(|child| child.is(NodeType::Begin)) {
            return;
        }
        let Some(tree) = parsed_tree(node) else {
            return;
        };
        if each_capture(&tree, Some(false)).is_empty() {
            return;
        }
        if each_capture(&tree, Some(true)).is_empty() {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}
