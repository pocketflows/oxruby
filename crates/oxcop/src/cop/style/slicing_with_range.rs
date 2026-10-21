//! Port of `lib/rubocop/cop/style/slicing_with_range.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static RANGE_FROM_ZERO_TILL_MINUS_ONE: NodePattern = NodePattern::new(
    "{
       (irange (int 0) {(int -1) nil})
       (erange (int 0) nil)
     }",
);

static RANGE_TILL_MINUS_ONE: NodePattern = NodePattern::new(
    "{
       (irange !nil? {(int -1) nil})
       (erange !nil? nil)
     }",
);

static RANGE_FROM_ZERO: NodePattern = NodePattern::new("(irange nil !nil?)");

#[derive(Default)]
pub struct SlicingWithRange;

impl Cop for SlicingWithRange {
    fn name(&self) -> &'static str {
        "Style/SlicingWithRange"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.6)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let args = node.arguments();
        if args.len() != 1 {
            return;
        }
        let range_node = args[0];
        let offense_range = find_offense_range(node);
        let Some((message, removal_range)) =
            offense_message_with_removal_range(cx, node, range_node, offense_range)
        else {
            return;
        };
        if removal_range != Some(offense_range) && unparenthesized_call(node) {
            return;
        }
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            if let Some(removal_range) = removal_range {
                corrector.remove(removal_range);
            }
        });
    }
}

fn unparenthesized_call(node: Node<'_>) -> bool {
    node.loc().dot.is_some() && !node.is_parenthesized()
}

fn find_offense_range(node: Node<'_>) -> Range {
    match node.loc().dot {
        Some(dot) => dot.join(node.source_range().end()),
        None => node.loc().selector.unwrap(),
    }
}

fn offense_message_with_removal_range(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    range_node: Node<'_>,
    offense_range: Range,
) -> Option<(String, Option<Range>)> {
    if RANGE_FROM_ZERO_TILL_MINUS_ONE.is_match(range_node) {
        Some((
            format!("Remove the useless `{}`.", cx.source(offense_range)),
            Some(offense_range),
        ))
    } else if RANGE_TILL_MINUS_ONE.is_match(range_node) {
        Some((
            offense_message_for_partial_range(cx, node, &endless(range_node), offense_range),
            range_node.child(1).map(|n| n.source_range()),
        ))
    } else if RANGE_FROM_ZERO.is_match(range_node) && cx.target_ruby_version() >= 2.7 {
        Some((
            offense_message_for_partial_range(cx, node, &beginless(range_node), offense_range),
            range_node.child(0).map(|n| n.source_range()),
        ))
    } else {
        None
    }
}

fn offense_message_for_partial_range(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    prefer: &str,
    offense_range: Range,
) -> String {
    let (current, prefer) = if node.loc().dot.is_some() {
        (arguments_source(node).to_owned(), prefer.to_owned())
    } else {
        (cx.source(offense_range).to_owned(), format!("[{prefer}]"))
    };
    format!("Prefer `{prefer}` over `{current}`.")
}

fn endless(range_node: Node<'_>) -> String {
    format!(
        "{}{}",
        range_node.child(0).unwrap().source(),
        range_node.range_source(range_node.loc().operator.unwrap())
    )
}

fn beginless(range_node: Node<'_>) -> String {
    format!(
        "{}{}",
        range_node.range_source(range_node.loc().operator.unwrap()),
        range_node.child(1).unwrap().source()
    )
}

fn arguments_source(node: Node<'_>) -> &str {
    let range = node
        .first_argument()
        .unwrap()
        .source_range()
        .join(node.last_argument().unwrap().source_range().end());
    node.range_source(range)
}
