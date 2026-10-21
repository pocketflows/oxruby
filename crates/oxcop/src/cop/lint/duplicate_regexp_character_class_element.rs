//! Port of `lib/rubocop/cop/lint/duplicate_regexp_character_class_element.rb`.

use std::collections::HashSet;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::parsed_tree;
use crate::ext::regexp_parser::ParsedTree;
use crate::processed_source::Range;
use crate::regexp_parser::{ExprId, Format, Tree};

const MSG_REPEATED_ELEMENT: &str = "Duplicate element inside regexp character class";

#[derive(Default)]
pub struct DuplicateRegexpCharacterClassElement;

impl Cop for DuplicateRegexpCharacterClassElement {
    fn name(&self) -> &'static str {
        "Lint/DuplicateRegexpCharacterClassElement"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for loc in each_repeated_character_class_element_loc(node) {
            cx.add_offense_with_correction(loc, MSG_REPEATED_ELEMENT, |corrector| {
                corrector.remove(loc);
            });
        }
    }
}

fn each_repeated_character_class_element_loc(node: Node<'_>) -> Vec<Range> {
    let mut locs = Vec::new();
    let Some(tree) = parsed_tree(node) else {
        return locs;
    };
    let interpolation_locs = interpolation_locs(node);
    for expr in tree.tree.each_expression(Tree::ROOT, false) {
        if skip_expression(&tree, expr) {
            continue;
        }
        let mut seen = HashSet::new();
        for &group in &tree.get(expr).expressions {
            if within_interpolation(&tree, &interpolation_locs, group) {
                continue;
            }
            let group_source = tree.tree.to_s(group, Format::Full);
            if seen.contains(&group_source) {
                locs.push(tree.expression(group));
            }
            seen.insert(group_source);
        }
    }
    locs
}

fn skip_expression(tree: &ParsedTree<'_>, expr: ExprId) -> bool {
    let exp = tree.get(expr);
    !exp.is_type("set") || exp.token == "intersection"
}

/// Since we blank interpolations with a space for every char of the interpolation, we would
/// mark every space (except the first) as duplicate if we do not skip regexp_parser nodes
/// that are within an interpolation.
fn within_interpolation(tree: &ParsedTree<'_>, interpolation_locs: &[Range], child: ExprId) -> bool {
    let parse_tree_child_loc = tree.expression(child);
    interpolation_locs
        .iter()
        .any(|il| il.overlaps(parse_tree_child_loc))
}

fn interpolation_locs(node: Node<'_>) -> Vec<Range> {
    node.child_nodes()
        .filter(|child| child.is(NodeType::Begin))
        .map(|child| child.source_range())
        .collect()
}
