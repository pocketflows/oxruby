//! Port of `lib/rubocop/cop/style/hash_as_last_array_item.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::mixin::util::{indent, same_line};
use crate::cop::{Cop, Corrector, Investigation};

#[derive(Default)]
pub struct HashAsLastArrayItem;

impl Cop for HashAsLastArrayItem {
    fn name(&self) -> &'static str {
        "Style/HashAsLastArrayItem"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Hash]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.child(0).is_some_and(|c| c.is(NodeType::Kwsplat)) {
            return;
        }
        let Some(array) = node.parent().filter(|p| p.is(NodeType::Array)) else {
            return;
        };
        let braces_style = style(cx) == "braces";
        if !(expected_braced_last_array_item(braces_style, array, node) && array.is_square_brackets())
        {
            return;
        }
        if braces_style {
            check_braces(cx, node);
        } else {
            check_no_braces(cx, node);
        }
    }
}

fn expected_braced_last_array_item(braces_style: bool, array: Node<'_>, node: Node<'_>) -> bool {
    let values: Vec<Node<'_>> = array.child_nodes().collect();
    if values.iter().all(|v| {
        v.is(NodeType::Hash)
            && if braces_style {
                v.has_braces()
            } else {
                !v.has_braces()
            }
    }) {
        return false;
    }
    let second_to_last_hash = values.len() >= 2 && values[values.len() - 2].is(NodeType::Hash);
    !second_to_last_hash && values.last() == Some(&node)
}

fn check_braces(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if node.has_braces() {
        return;
    }
    let ps = cx.processed_source;
    cx.add_offense_with_correction(node.source_range(), "Wrap hash in `{` and `}`.", |corrector| {
        let parent = node.parent().unwrap();
        if node.is_single_line() || same_line(ps, node.source_range(), parent.source_range()) {
            corrector.wrap(node.source_range(), "{", "}");
        } else {
            let indent = indent(node, 0);
            corrector.wrap(
                node.source_range(),
                format!("{{\n{indent}"),
                format!("\n{indent}}}"),
            );
        }
    });
}

fn check_no_braces(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !node.has_braces() || node.children().is_empty() {
        return;
    }
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(
        node.source_range(),
        "Omit the braces around the hash.",
        |corrector| {
            remove_last_element_trailing_comma(src, corrector, node.parent().unwrap());
            corrector.remove(node.loc().begin.unwrap());
            corrector.remove(node.loc().end.unwrap());
        },
    );
}

fn remove_last_element_trailing_comma(src: &str, corrector: &mut Corrector, node: Node<'_>) {
    let last = node.child_nodes().last().unwrap();
    let range = range_with_surrounding_space(
        src,
        last.source_range(),
        SurroundingSpace::side(Side::Right),
    )
    .end()
    .resize(1);
    if src.get(range.begin_pos..range.end_pos) == Some(",") {
        corrector.remove(range);
    }
}
