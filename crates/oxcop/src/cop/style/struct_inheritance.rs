//! Port of `lib/rubocop/cop/style/struct_inheritance.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{
    range_between, range_by_whole_lines, range_with_surrounding_space, Side, SurroundingSpace,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str =
    "Don't extend an instance initialized by `Struct.new`. Use a block to customize the struct.";

static STRUCT_CONSTRUCTOR: NodePattern = NodePattern::new(
    "{(send (const {nil? cbase} :Struct) :new ...)
      (block (send (const {nil? cbase} :Struct) :new ...) ...)}",
);

#[derive(Default)]
pub struct StructInheritance;

impl Cop for StructInheritance {
    fn name(&self) -> &'static str {
        "Style/StructInheritance"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(parent_class) = node.parent_class() else {
            return;
        };
        if !STRUCT_CONSTRUCTOR.is_match(parent_class) || defines_constants(node.body()) {
            return;
        }
        let ps = cx.processed_source;
        cx.add_offense_with_correction(parent_class.source_range(), MSG, |corrector| {
            let src = ps.buffer_source();
            corrector.remove(range_with_surrounding_space(
                src,
                node.loc().keyword.unwrap(),
                SurroundingSpace {
                    side: Side::Right,
                    newlines: false,
                    ..SurroundingSpace::default()
                },
            ));
            corrector.replace(node.loc().operator.unwrap(), "=");
            correct_parent(ps, parent_class, corrector);
        });
    }
}

fn defines_constants(class_body: Option<Node<'_>>) -> bool {
    let Some(class_body) = class_body else {
        return false;
    };
    class_body
        .each_node()
        .any(|n| n.is_any(&[NodeType::Casgn, NodeType::Class, NodeType::Module]))
}

fn correct_parent(ps: &ProcessedSource<'_>, parent: Node<'_>, corrector: &mut Corrector) {
    let class_node = parent.parent().unwrap();
    if parent.is(NodeType::Block) {
        corrector.remove(range_with_surrounding_space(
            ps.buffer_source(),
            parent.loc().end.unwrap(),
            SurroundingSpace {
                newlines: false,
                ..SurroundingSpace::default()
            },
        ));
    } else if class_node.body().is_none() {
        corrector.remove(range_for_empty_class_body(ps, class_node, parent));
    } else if unparenthesized_struct_new(parent) {
        wrap_unparenthesized_call_with_do(corrector, parent);
    } else {
        corrector.insert_after(parent.source_range(), " do");
    }
}

fn range_for_empty_class_body(
    ps: &ProcessedSource<'_>,
    class_node: Node<'_>,
    struct_new: Node<'_>,
) -> crate::processed_source::Range {
    if class_node.is_single_line() {
        range_between(
            struct_new.source_range().end_pos,
            class_node.source_range().end_pos,
        )
    } else {
        range_by_whole_lines(ps, class_node.loc().end.unwrap(), true)
    }
}

fn unparenthesized_struct_new(parent: Node<'_>) -> bool {
    parent.is_send() && parent.has_arguments() && !parent.is_parenthesized()
}

fn wrap_unparenthesized_call_with_do(corrector: &mut Corrector, parent: Node<'_>) {
    let args_source = parent
        .arguments()
        .iter()
        .map(|a| a.source())
        .collect::<Vec<_>>()
        .join(", ");
    let range = parent
        .loc()
        .selector
        .unwrap()
        .end()
        .join(parent.source_range().end());
    corrector.replace(range, format!("({args_source}) do"));
}
