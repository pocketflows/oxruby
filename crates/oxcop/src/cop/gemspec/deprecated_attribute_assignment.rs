//! Port of `lib/rubocop/cop/gemspec/deprecated_attribute_assignment.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};

static GEM_SPECIFICATION: NodePattern = NodePattern::new(
    "(block
       (send
         (const
           (const {cbase nil?} :Gem) :Specification) :new)
       ...)",
);

const DEPRECATED_ATTRIBUTES: &[&str] = &[
    "test_files",
    "date",
    "specification_version",
    "rubygems_version",
];

#[derive(Default)]
pub struct DeprecatedAttributeAssignment;

impl Cop for DeprecatedAttributeAssignment {
    fn name(&self) -> &'static str {
        "Gemspec/DeprecatedAttributeAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, block_node: Node<'a>) {
        if !GEM_SPECIFICATION.is_match(block_node) {
            return;
        }
        let Some(block_parameter) = block_node.first_argument().map(|a| a.source()) else {
            return;
        };
        let Some((assignment, attribute)) = block_node
            .descendants()
            .find_map(|node| use_deprecated_attributes(node, block_parameter).map(|a| (node, a)))
        else {
            return;
        };
        let message = format!("Do not set `{attribute}` in gemspec.");
        let range = range_by_whole_lines(cx.processed_source, assignment.source_range(), true);
        cx.add_offense_with_correction(assignment.source_range(), &message, |corrector| {
            corrector.remove(range);
        });
    }
}

/// The deprecated attribute `node` sets, if any.
fn use_deprecated_attributes(node: Node<'_>, block_parameter: &str) -> Option<&'static str> {
    DEPRECATED_ATTRIBUTES.iter().copied().find(|attribute| {
        let (node, method_name) = if node.is(NodeType::OpAsgn) {
            match node.lhs() {
                Some(lhs) => (lhs, (*attribute).to_owned()),
                None => return false,
            }
        } else {
            (node, format!("{attribute}="))
        };
        node.is(NodeType::Send)
            && node.receiver().is_some_and(|r| r.source() == block_parameter)
            && node.is_method(&method_name)
    })
}
