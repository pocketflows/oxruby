//! Port of `lib/rubocop/cop/lint/redundant_regexp_quantifiers.rb`.

use std::collections::HashSet;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::parsed_tree;
use crate::ext::regexp_parser::ParsedTree;
use crate::regexp_parser::{ExprId, Tree};

#[derive(Default)]
pub struct RedundantRegexpQuantifiers;

impl Cop for RedundantRegexpQuantifiers {
    fn name(&self) -> &'static str {
        "Lint/RedundantRegexpQuantifiers"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Regexp]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.child_nodes().any(|c| c.is(NodeType::Begin)) {
            return;
        }
        let Some(tree) = parsed_tree(node) else {
            return;
        };

        for (group, child) in each_redundantly_quantified_pair(&tree) {
            let replacement = merged_quantifier(&tree.tree, group, child);
            let (Some(group_q), Some(child_q)) = (tree.quantifier(group), tree.quantifier(child))
            else {
                continue;
            };
            let message = format!(
                "Replace redundant quantifiers `{}` and `{}` with a single `{replacement}`.",
                quantifier_text(&tree.tree, child),
                quantifier_text(&tree.tree, group)
            );
            cx.add_offense_with_correction(
                range_between(child_q.begin_pos, group_q.end_pos),
                &message,
                |corrector| {
                    corrector.replace(group_q, "");
                    corrector.replace(child_q, replacement);
                },
            );
        }
    }
}

fn quantifier_text(tree: &Tree, id: ExprId) -> &str {
    tree.get(id).quantifier.as_ref().map_or("", |q| q.text.as_str())
}

fn each_redundantly_quantified_pair(tree: &ParsedTree<'_>) -> Vec<(ExprId, ExprId)> {
    let tree = &tree.tree;
    let mut pairs = Vec::new();
    let mut seen = HashSet::new();
    for expr in tree.each_expression(Tree::ROOT, false) {
        if seen.contains(&expr) || !redundant_group(tree, expr) || mergeable_quantifier(tree, expr).is_none()
        {
            continue;
        }
        for subexp in tree.each_expression(expr, false) {
            seen.insert(subexp);
            if !redundantly_quantifiable(tree, subexp) {
                break;
            }
            if mergeable_quantifier(tree, subexp).is_some() {
                pairs.push((expr, subexp));
            }
        }
    }
    pairs
}

fn redundant_group(tree: &Tree, expr: ExprId) -> bool {
    let exp = tree.get(expr);
    exp.is("passive", Some("group"))
        && exp
            .expressions
            .iter()
            .filter(|&&child| tree.get(child).ty != "free_space")
            .count()
            == 1
}

fn redundantly_quantifiable(tree: &Tree, node: ExprId) -> bool {
    redundant_group(tree, node) || character_set(tree, node) || tree.get(node).is_terminal()
}

fn character_set(tree: &Tree, expr: ExprId) -> bool {
    tree.get(expr).is("character", Some("set"))
}

fn mergeable_quantifier(tree: &Tree, expr: ExprId) -> Option<&'static str> {
    let exp = tree.get(expr);
    if !exp.is_greedy() {
        return None;
    }
    match exp.quantity()? {
        (0, -1) => Some("*"),
        (0, 1) => Some("?"),
        (1, -1) => Some("+"),
        _ => None,
    }
}

fn merged_quantifier(tree: &Tree, exp1: ExprId, exp2: ExprId) -> &'static str {
    let quantifier1 = mergeable_quantifier(tree, exp1);
    let quantifier2 = mergeable_quantifier(tree, exp2);
    if quantifier1 == quantifier2 {
        quantifier1.unwrap_or("*")
    } else {
        "*"
    }
}
