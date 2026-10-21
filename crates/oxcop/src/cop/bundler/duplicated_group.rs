//! Port of `lib/rubocop/cop/bundler/duplicated_group.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};

const SOURCE_BLOCK_NAMES: &[&str] = &["source", "git", "platforms", "path"];

static GROUP_DECLARATIONS: NodePattern = NodePattern::new("(send nil? :group ...)");

#[derive(Default)]
pub struct DuplicatedGroup;

impl Cop for DuplicatedGroup {
    fn name(&self) -> &'static str {
        "Bundler/DuplicatedGroup"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let Some(ast) = cx.processed_source.ast() else {
            return;
        };
        for nodes in duplicated_group_nodes(ast) {
            let first_line = nodes[0].first_line();
            for node in &nodes[1..] {
                let group_name = node
                    .arguments()
                    .iter()
                    .map(|a| a.source())
                    .collect::<Vec<_>>()
                    .join(", ");
                register_offense(cx, *node, &group_name, first_line);
            }
        }
    }
}

fn duplicated_group_nodes(ast: Node<'_>) -> Vec<Vec<Node<'_>>> {
    let mut groups: Vec<(String, Vec<Node<'_>>)> = Vec::new();
    for (node, _) in GROUP_DECLARATIONS.search(ast) {
        let source_key = find_source_key(node).unwrap_or_default();
        let mut attributes = group_attributes(node);
        attributes.sort();
        let key = format!("{source_key}{}", attributes.concat());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, nodes)) => nodes.push(node),
            None => groups.push((key, vec![node])),
        }
    }
    groups
        .into_iter()
        .map(|(_, nodes)| nodes)
        .filter(|nodes| nodes.len() > 1)
        .collect()
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    group_name: &str,
    line_of_first_occurrence: usize,
) {
    let length = node.last_column().saturating_sub(node.column());
    let offense_location = source_range(cx.processed_source, node.first_line(), node.column(), length);
    let message = format!(
        "Gem group `{group_name}` already defined on line {line_of_first_occurrence} of the Gemfile."
    );
    cx.add_offense(offense_location, &message);
}

fn find_source_key(node: Node<'_>) -> Option<String> {
    let source_block = node.ancestors().filter(|a| a.is(NodeType::Block)).find(|block| {
        block
            .method_name()
            .is_some_and(|name| SOURCE_BLOCK_NAMES.contains(&name))
    })?;
    let argument = source_block
        .send_node()
        .and_then(|s| s.first_argument())
        .map_or("", |a| a.source());
    Some(format!("{}{argument}", source_block.method_name().unwrap_or("")))
}

fn group_attributes(node: Node<'_>) -> Vec<String> {
    node.arguments()
        .iter()
        .map(|argument| {
            if argument.is(NodeType::Hash) {
                let mut pairs: Vec<&str> = argument.pairs().iter().map(|p| p.source()).collect();
                pairs.sort();
                pairs.join(", ")
            } else if responds_to_value(*argument) {
                value_to_s(*argument)
            } else {
                argument.source().to_owned()
            }
        })
        .collect()
}

/// Node classes with a `value` method (`BasicLiteralNode` and friends).
fn responds_to_value(node: Node<'_>) -> bool {
    use NodeType::*;
    matches!(node.ty(), Str | Sym | Int | Float | Rational | Complex | Dstr | Xstr | Dsym)
}

/// `value.to_s`.
fn value_to_s(node: Node<'_>) -> String {
    match node.children().first() {
        Some(Child::Sym(s) | Child::Str(s)) if matches!(node.ty(), NodeType::Str | NodeType::Sym) => {
            s.clone()
        }
        Some(Child::Int(i)) => i.to_string(),
        _ => node.source().to_owned(),
    }
}
