//! Port of `lib/rubocop/cop/correctors/each_to_for_corrector.rb`: rewrites
//! `xs.each do |x|` to `for x in xs do`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::processed_source::Range;

pub struct EachToForCorrector<'a> {
    block_node: Node<'a>,
    collection_node: Node<'a>,
    /// `block_node.arguments` (`nil` for numbered and `it` blocks).
    argument_node: Option<Node<'a>>,
}

impl<'a> EachToForCorrector<'a> {
    pub fn new(block_node: Node<'a>) -> Self {
        Self {
            block_node,
            collection_node: block_node.receiver().unwrap(),
            argument_node: block_node
                .is(NodeType::Block)
                .then(|| block_node.child(1))
                .flatten(),
        }
    }

    pub fn call(&self, corrector: &mut Corrector) {
        corrector.replace(self.offending_range(), self.correction());
    }

    fn has_arguments(&self) -> bool {
        self.argument_node.is_some_and(|a| !a.children().is_empty())
    }

    fn correction(&self) -> String {
        if self.has_arguments() {
            let variables: Vec<&str> = self
                .argument_node
                .unwrap()
                .child_nodes()
                .map(|c| c.source())
                .collect();
            format!(
                "for {} in {} do",
                variables.join(", "),
                self.collection_node.source()
            )
        } else {
            format!("for _ in {} do", self.collection_node.source())
        }
    }

    fn offending_range(&self) -> Range {
        let begin_range = self.block_node.source_range().begin();
        if self.has_arguments() {
            begin_range.join(self.argument_node.unwrap().source_range().end())
        } else {
            begin_range.join(self.block_node.loc().begin.unwrap().end())
        }
    }
}
