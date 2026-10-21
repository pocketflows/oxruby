//! Port of `lib/rubocop/cop/style/map_compact_with_conditional_block.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static CONDITIONAL_BLOCK: NodePattern = NodePattern::new(
    "(block
      (call _ {:map :filter_map})
      (args
        $(arg _))
      {
        (if $_ $(lvar _) {next nil?})
        (if $_ {next nil?} $(lvar _))
        (if $_ (next $(lvar _)) {next nil nil?})
        (if $_ {next nil nil?} (next $(lvar _)))
        (begin
          {
            (if $_ next nil?)
            (if $_ nil? next)
          }
          $(lvar _))
        (begin
          {
            (if $_ (next $(lvar _)) nil?)
            (if $_ nil? (next $(lvar _)))
          }
          (nil))
      })",
);

#[derive(Default)]
pub struct MapCompactWithConditionalBlock {
    ignored: IgnoredNodes,
}

fn conditional_block(node: Option<Node<'_>>) -> Option<(Node<'_>, Node<'_>, Node<'_>)> {
    let captures = CONDITIONAL_BLOCK.match_node(node?)?;
    Some((captures[0].node()?, captures[1].node()?, captures[2].node()?))
}

impl Cop for MapCompactWithConditionalBlock {
    fn name(&self) -> &'static str {
        "Style/MapCompactWithConditionalBlock"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["compact", "filter_map"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let (block_argument, condition, return_value, range) =
            if let Some((a, c, r)) = conditional_block(node.child(0)) {
                if !(node.is_method("compact") && node.arguments().is_empty()) {
                    return;
                }
                (a, c, r, map_with_compact_range(node))
            } else if let Some((a, c, r)) = conditional_block(node.parent()) {
                if !node.is_method("filter_map") {
                    return;
                }
                (a, c, r, filter_map_range(node))
            } else {
                return;
            };
        self.inspect(cx, node, block_argument, condition, return_value, range);
    }
}

impl MapCompactWithConditionalBlock {
    fn inspect(
        &mut self,
        cx: &mut Investigation<'_, '_>,
        node: Node<'_>,
        block_argument_node: Node<'_>,
        condition_node: Node<'_>,
        return_value_node: Node<'_>,
        range: Range,
    ) {
        if block_argument_node.name() != return_value_node.child_sym(0) {
            return;
        }
        if condition_node.parent().is_some_and(|p| p.is_elsif()) {
            return;
        }
        // The correction block `return`s out of `inspect` (so no offense is
        // added) when the node is ignored.
        if self.ignored.part_of(node) || self.ignored.contains(node) {
            return;
        }
        let method = if truthy_branch(return_value_node) {
            "select"
        } else {
            "reject"
        };
        let current = if node.is_method("compact") {
            format!(
                "{} {{ ... }}.compact",
                node.child(0).and_then(|m| m.method_name()).unwrap_or("")
            )
        } else {
            "filter_map { ... }".to_owned()
        };
        let message = format!("Replace `{current}` with `{method}`.");
        let replacement = format!(
            "{method} {{ |{}| {} }}",
            block_argument_node.source(),
            condition_node.source()
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, &replacement);
        });
        self.ignored.ignore(node);
    }
}

fn truthy_branch(node: Node<'_>) -> bool {
    let parent = node.parent().unwrap();
    if parent.is(NodeType::Begin) {
        truthy_branch_for_guard(node)
    } else if parent.is(NodeType::Next) {
        truthy_branch_for_if(parent)
    } else {
        truthy_branch_for_if(node)
    }
}

fn truthy_branch_for_if(node: Node<'_>) -> bool {
    let if_node = node.parent().unwrap();
    if if_node.is_if() || if_node.is_ternary() {
        if_node.if_branch() == Some(node)
    } else if if_node.is_unless() {
        if_node.else_branch() == Some(node)
    } else {
        false
    }
}

fn truthy_branch_for_guard(node: Node<'_>) -> bool {
    let Some(if_node) = node.left_sibling() else {
        return false;
    };
    let has_arguments = if_node
        .if_branch()
        .is_some_and(|b| !b.arguments().is_empty());
    if if_node.is_if() {
        has_arguments
    } else {
        !has_arguments
    }
}

fn map_with_compact_range(node: Node<'_>) -> Range {
    node.receiver()
        .unwrap()
        .send_node()
        .unwrap()
        .loc()
        .selector
        .unwrap()
        .begin()
        .join(node.source_range().end())
}

fn filter_map_range(node: Node<'_>) -> Range {
    node.loc()
        .selector
        .unwrap()
        .join(node.parent().unwrap().source_range().end())
}
