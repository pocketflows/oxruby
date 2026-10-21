//! Port of `lib/rubocop/cop/correctors/for_to_each_corrector.rb`: rewrites
//! `for x in xs do` to `xs.each do |x|`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::processed_source::Range;

pub struct ForToEachCorrector<'a> {
    for_node: Node<'a>,
    variable_node: Node<'a>,
    collection_node: Node<'a>,
}

impl<'a> ForToEachCorrector<'a> {
    pub fn new(for_node: Node<'a>) -> Self {
        Self {
            for_node,
            variable_node: for_node.child(0).unwrap(),
            collection_node: for_node.child(1).unwrap(),
        }
    }

    pub fn call(&self, corrector: &mut Corrector) {
        let offending_range = self.for_node.source_range().begin().join(self.end_range());
        corrector.replace(offending_range, self.correction());
    }

    fn correction(&self) -> String {
        let dot = if self.collection_node.is(NodeType::Csend) {
            "&."
        } else {
            "."
        };
        format!(
            "{}{dot}each do |{}|",
            self.collection_source(),
            self.variable_node.source()
        )
    }

    fn collection_source(&self) -> String {
        if self.requires_parentheses() {
            format!("({})", self.collection_node.source())
        } else {
            self.collection_node.source().to_owned()
        }
    }

    fn requires_parentheses(&self) -> bool {
        let collection = self.collection_node;
        if collection.is(NodeType::Send) && collection.is_operator_method() {
            return true;
        }
        collection.is_range() || collection.is_operator_keyword()
    }

    fn end_range(&self) -> Range {
        if self.for_node.has_do() {
            self.for_node.loc().begin.unwrap().end()
        } else {
            self.collection_end().end()
        }
    }

    fn collection_end(&self) -> Range {
        if self.collection_node.is(NodeType::Begin) {
            self.collection_node.loc().end.unwrap()
        } else {
            self.collection_node.source_range()
        }
    }
}
