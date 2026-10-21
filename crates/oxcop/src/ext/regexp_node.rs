//! Port of `lib/rubocop/ext/regexp_node.rb`: `RegexpNode#parsed_tree` and
//! `#each_capture`.

use crate::ast::{Node, NodeType};
use crate::ext::regexp_parser::ParsedTree;
use crate::regexp_parser::{self, Class, Event, ExprId, GroupKind};

/// `with_interpolations_blanked`: the regexp's source with each
/// interpolation replaced by spaces.
pub fn with_interpolations_blanked(node: Node<'_>) -> String {
    let children: Vec<Node<'_>> = node.child_nodes().collect();
    let parts = &children[..children.len().saturating_sub(1)];
    parts
        .iter()
        .map(|child| {
            let source = child.source();
            if child.is(NodeType::Begin) {
                " ".repeat(source.chars().count())
            } else {
                source.to_owned()
            }
        })
        .collect()
}

/// `RegexpNode#options` includes `Regexp::EXTENDED`.
fn extended(node: Node<'_>) -> bool {
    node.child_nodes()
        .last()
        .filter(|regopt| regopt.is(NodeType::Regopt))
        .is_some_and(|regopt| regopt.children().iter().any(|c| c.as_sym() == Some("x")))
}

/// `parsed_tree`: `None` when regexp_parser can't parse the regexp.
pub fn parsed_tree<'a>(node: Node<'a>) -> Option<ParsedTree<'a>> {
    if !node.is(NodeType::Regexp) {
        return None;
    }
    let str = with_interpolations_blanked(node);
    // RuboCop always passes `options:`, so the scanner always runs in binary
    // mode (`options & Regexp::NOENCODING` is 0, which Ruby treats as true).
    let tree = regexp_parser::parse(&str, extended(node), true).ok()?;
    let origin = node.loc().begin?.end_pos;
    Some(ParsedTree::new(tree, node.ast().source(), origin))
}

/// `each_capture(named:)`: capturing groups, named ones or numbered ones.
pub fn each_capture(tree: &ParsedTree<'_>, named: Option<bool>) -> Vec<ExprId> {
    tree.tree
        .traverse(regexp_parser::Tree::ROOT, false)
        .into_iter()
        .filter(|&(event, exp)| {
            let e = tree.tree.get(exp);
            let is_named = e.class == Class::Group(GroupKind::Named)
                || matches!(
                    e.class,
                    Class::Backreference(
                        regexp_parser::expression::BackrefKind::Name
                            | regexp_parser::expression::BackrefKind::NameCall
                            | regexp_parser::expression::BackrefKind::NameRecursionLevel
                    )
                );
            event == Event::Enter && named.is_none_or(|n| n == is_named) && e.is_capturing()
        })
        .map(|(_, exp)| exp)
        .collect()
}
