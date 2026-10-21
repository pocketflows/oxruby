//! Port of `lib/rubocop/cop/bundler/duplicated_gem.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};

static GEM_DECLARATIONS: NodePattern = NodePattern::new("(send nil? :gem str ...)");

#[derive(Default)]
pub struct DuplicatedGem;

impl Cop for DuplicatedGem {
    fn name(&self) -> &'static str {
        "Bundler/DuplicatedGem"
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let Some(ast) = cx.processed_source.ast() else {
            return;
        };
        for nodes in duplicated_gem_nodes(ast) {
            let first_line = nodes[0].first_line();
            for node in &nodes[1..] {
                let gem_name = node.first_argument().and_then(|a| a.str_content()).unwrap_or("");
                register_offense(cx, *node, gem_name, first_line);
            }
        }
    }
}

fn duplicated_gem_nodes(ast: Node<'_>) -> Vec<Vec<Node<'_>>> {
    let mut groups: Vec<(Node<'_>, Vec<Node<'_>>)> = Vec::new();
    for (node, _) in GEM_DECLARATIONS.search(ast) {
        let key = node.first_argument().unwrap();
        match groups.iter_mut().find(|(k, _)| k.same_as(key)) {
            Some((_, nodes)) => nodes.push(node),
            None => groups.push((key, vec![node])),
        }
    }
    groups
        .into_iter()
        .map(|(_, nodes)| nodes)
        .filter(|nodes| nodes.len() > 1 && !conditional_declaration(nodes))
        .collect()
}

fn conditional_declaration(nodes: &[Node<'_>]) -> bool {
    let Some(parent) = nodes[0].ancestors().find(|a| !a.is(NodeType::Begin)) else {
        return false;
    };
    if !matches!(parent.ty(), NodeType::If | NodeType::When) {
        return false;
    }
    let root_conditional_node = if parent.is(NodeType::If) {
        parent
    } else {
        match parent.parent() {
            Some(p) => p,
            None => return false,
        }
    };
    nodes
        .iter()
        .all(|node| within_conditional(*node, root_conditional_node))
}

fn within_conditional(node: Node<'_>, conditional_node: Node<'_>) -> bool {
    conditional_node.branches().into_iter().flatten().any(|branch| {
        branch.same_as(node) || branch.child_nodes().any(|c| c.same_as(node))
    })
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    gem_name: &str,
    line_of_first_occurrence: usize,
) {
    let length = node.last_column().saturating_sub(node.column());
    let offense_location = source_range(cx.processed_source, node.first_line(), node.column(), length);
    let message = format!(
        "Gem `{gem_name}` requirements already given on line {line_of_first_occurrence} of the Gemfile."
    );
    cx.add_offense(offense_location, &message);
}
