//! Port of `lib/rubocop/cop/lint/mixed_case_range.rb`.

use std::ops::RangeInclusive;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::parsed_tree;
use crate::ext::regexp_parser::ParsedTree;
use crate::processed_source::Range;
use crate::regexp_parser::{ExprId, Tree};

const MSG: &str = "Ranges from upper to lower case ASCII letters may include unintended \
                   characters. Instead of `A-z` (which also includes several symbols) \
                   specify each range individually: `A-Za-z` and individually specify any symbols.";
const RANGES: [RangeInclusive<char>; 2] = ['a'..='z', 'A'..='Z'];

#[derive(Default)]
pub struct MixedCaseRange;

impl Cop for MixedCaseRange {
    fn name(&self) -> &'static str {
        "Lint/MixedCaseRange"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Irange, NodeType::Erange, NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Regexp) {
            on_regexp(cx, node);
        } else {
            on_irange(cx, node);
        }
    }
}

fn on_irange(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    if !node.child_nodes().all(|c| c.is(NodeType::Str)) {
        return;
    }
    let (Some(range_begin), Some(range_end)) = (node.child(0), node.child(1)) else {
        return;
    };
    let (Some(range_start), Some(range_end)) = (range_begin.str_content(), range_end.str_content())
    else {
        return;
    };
    if unsafe_range(range_start, range_end) {
        cx.add_offense(node.source_range(), MSG);
    }
}

fn on_regexp(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    for loc in each_unsafe_regexp_range(node) {
        let Some(replacement) = regexp_range(node.range_source(loc)) else {
            continue;
        };
        cx.add_offense_with_correction(loc, MSG, |corrector| {
            corrector.replace(loc, replacement);
        });
    }
}

fn each_unsafe_regexp_range(node: Node<'_>) -> Vec<Range> {
    let mut locs = Vec::new();
    let Some(tree) = parsed_tree(node) else {
        return locs;
    };
    for expr in tree.tree.each_expression(Tree::ROOT, false) {
        if skip_expression(&tree, expr) {
            continue;
        }
        for (range_start, range_end) in range_pairs(&tree, expr) {
            if skip_range(&tree, range_start, range_end) {
                continue;
            }
            let (Some(range_start), Some(range_end)) = (range_start, range_end) else {
                continue;
            };
            if !unsafe_range(&tree.get(range_start).text, &tree.get(range_end).text) {
                continue;
            }
            locs.push(build_source_range(&tree, range_start, range_end));
        }
    }
    locs
}

fn build_source_range(tree: &ParsedTree<'_>, range_start: ExprId, range_end: ExprId) -> Range {
    range_between(
        tree.expression(range_start).begin_pos,
        tree.expression(range_end).end_pos,
    )
}

fn range_for(char: &str) -> Option<&'static RangeInclusive<char>> {
    let mut chars = char.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    RANGES.iter().find(|range| range.contains(&c))
}

fn range_pairs(tree: &ParsedTree<'_>, expr: ExprId) -> Vec<(Option<ExprId>, Option<ExprId>)> {
    tree.get(expr)
        .expressions
        .iter()
        .map(|&e| tree.get(e))
        .filter(|e| e.is_type("set"))
        .map(|e| (e.expressions.first().copied(), e.expressions.get(1).copied()))
        .collect()
}

fn unsafe_range(range_start: &str, range_end: &str) -> bool {
    if range_start.chars().count() != 1 || range_end.chars().count() != 1 {
        return false;
    }
    range_for(range_start) != range_for(range_end)
}

fn skip_expression(tree: &ParsedTree<'_>, expr: ExprId) -> bool {
    let exp = tree.get(expr);
    !(exp.is_type("set") && exp.token == "character")
}

fn skip_range(tree: &ParsedTree<'_>, range_start: Option<ExprId>, range_end: Option<ExprId>) -> bool {
    [range_start, range_end]
        .iter()
        .any(|bound| !bound.is_some_and(|b| tree.get(b).is_type("literal")))
}

fn regexp_range(source: &str) -> Option<String> {
    let mut parts = source.split('-');
    let open = parts.next().unwrap_or("");
    let close = parts.next().unwrap_or("");
    let open_range = range_for(open)?;
    let close_range = range_for(close)?;
    let first = uniq_join(open, &open_range.end().to_string());
    let second = uniq_join(&close_range.start().to_string(), close);
    Some(format!("{first}{second}"))
}

/// `[a, b].uniq.join('-')`
fn uniq_join(a: &str, b: &str) -> String {
    if a == b { a.to_owned() } else { format!("{a}-{b}") }
}
