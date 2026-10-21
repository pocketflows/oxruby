//! Port of `lib/rubocop/cop/style/data_inheritance.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{
    SurroundingSpace, range_between, range_by_whole_lines, range_with_surrounding_space,
};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str =
    "Don't extend an instance initialized by `Data.define`. Use a block to customize the class.";

static DATA_DEFINE: NodePattern = NodePattern::new(
    "{(send (const {nil? cbase} :Data) :define ...)
      (block (send (const {nil? cbase} :Data) :define ...) ...)}",
);

#[derive(Default)]
pub struct DataInheritance;

impl Cop for DataInheritance {
    fn name(&self) -> &'static str {
        "Style/DataInheritance"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.2)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(parent_class) = node.parent_class().filter(|p| DATA_DEFINE.is_match(*p)) else {
            return;
        };
        if defines_constants(node.body()) {
            return;
        }
        let ps = cx.processed_source;
        cx.add_offense_with_correction(parent_class.source_range(), MSG, |corrector| {
            let no_newlines = SurroundingSpace {
                newlines: false,
                ..SurroundingSpace::default()
            };
            corrector.remove(range_with_surrounding_space(
                ps.buffer_source(),
                node.loc().keyword.unwrap(),
                no_newlines,
            ));
            corrector.replace(node.loc().operator.unwrap(), "=");
            correct_parent(ps, parent_class, corrector);
        });
    }
}

fn defines_constants(class_body: Option<Node<'_>>) -> bool {
    class_body.is_some_and(|body| {
        body.each_node().any(|n| {
            matches!(
                n.ty(),
                NodeType::Casgn | NodeType::Class | NodeType::Module
            )
        })
    })
}

fn correct_parent(ps: &ProcessedSource<'_>, parent: Node<'_>, corrector: &mut Corrector) {
    if parent.is(NodeType::Block) {
        if parent.has_braces() {
            corrector.replace(parent.loc().begin.unwrap(), "do");
        }
        let no_newlines = SurroundingSpace {
            newlines: false,
            ..SurroundingSpace::default()
        };
        corrector.remove(range_with_surrounding_space(
            ps.buffer_source(),
            parent.loc().end.unwrap(),
            no_newlines,
        ));
    } else {
        let class_node = parent.parent().unwrap();
        if class_node.body().is_none() {
            corrector.remove(range_for_empty_class_body(ps, class_node, parent));
        } else {
            corrector.insert_after(parent.source_range(), " do");
        }
    }
}

fn range_for_empty_class_body(
    ps: &ProcessedSource<'_>,
    class_node: Node<'_>,
    data_define: Node<'_>,
) -> Range {
    if class_node.is_single_line() {
        range_between(
            data_define.source_range().end_pos,
            class_node.source_range().end_pos,
        )
    } else {
        range_by_whole_lines(ps, class_node.loc().end.unwrap(), true)
    }
}
