//! Port of `lib/rubocop/cop/style/mixin_grouping.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::util::indent;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MIXIN_METHODS: &[&str] = &["extend", "include", "prepend"];

#[derive(Default)]
pub struct MixinGrouping;

impl Cop for MixinGrouping {
    fn name(&self) -> &'static str {
        "Style/MixinGrouping"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let begin_node = node
            .child_nodes()
            .find(|c| c.is(NodeType::Begin))
            .unwrap_or(node);
        let macros: Vec<Node<'a>> = begin_node
            .child_nodes()
            .filter(|c| c.is(NodeType::Send) && c.is_macro())
            .collect();
        for macro_node in macros {
            if !macro_node
                .method_name()
                .is_some_and(|m| MIXIN_METHODS.contains(&m))
                || macro_node.arguments().is_empty()
            {
                continue;
            }
            if style(cx) == "separated" {
                check_separated_style(cx, macro_node);
            } else {
                check_grouped_style(cx, macro_node);
            }
        }
    }
}

fn range_to_remove_for_subsequent_mixin(
    cx: &Investigation<'_, '_>,
    mixins: &[Node<'_>],
    node: Node<'_>,
) -> Range {
    let mut range = node.source_range();
    let Some(prev_mixin) = mixins.windows(2).find(|w| w[1] == node).map(|w| w[0]) else {
        return range;
    };
    let between = prev_mixin.source_range().end().join(range.begin());
    // If separated from previous mixin with only whitespace, remove that too.
    if cx.source(between).chars().all(char::is_whitespace) {
        range = range.join(between);
    }
    range
}

fn check_grouped_style(cx: &mut Investigation<'_, '_>, send_node: Node<'_>) {
    let mixins = sibling_mixins(send_node);
    if mixins.len() == 1 {
        return;
    }
    let message = format!(
        "Put `{}` mixins in a single statement.",
        send_node.method_name().unwrap_or("")
    );
    cx.add_offense_cx(send_node.source_range(), &message, |corrector, cx| {
        let (range, correction) = if mixins.first() == Some(&send_node) {
            (send_node.source_range(), group_mixins(send_node, &mixins))
        } else {
            (
                range_to_remove_for_subsequent_mixin(cx, &mixins, send_node),
                String::new(),
            )
        };
        corrector.replace(range, correction);
    });
}

fn check_separated_style(cx: &mut Investigation<'_, '_>, send_node: Node<'_>) {
    if send_node.arguments().len() == 1 {
        return;
    }
    let message = format!(
        "Put `{}` mixins in separate statements.",
        send_node.method_name().unwrap_or("")
    );
    cx.add_offense_with_correction(send_node.source_range(), &message, |corrector| {
        corrector.replace(send_node.source_range(), separate_mixins(send_node));
    });
}

fn sibling_mixins(send_node: Node<'_>) -> Vec<Node<'_>> {
    let Some(parent) = send_node.parent() else {
        return vec![send_node];
    };
    parent
        .child_nodes()
        .filter(|s| s.is(NodeType::Send) && s.is_macro())
        .filter(|s| s.method_name() == send_node.method_name())
        .collect()
}

fn separate_mixins(node: Node<'_>) -> String {
    let mut arguments = node.arguments();
    arguments.reverse();
    let method_name = node.method_name().unwrap_or("");
    let mut mixins = vec![format!("{method_name} {}", arguments[0].source())];
    for arg in &arguments[1..] {
        mixins.push(format!("{}{method_name} {}", indent(node, 0), arg.source()));
    }
    mixins.join("\n")
}

fn group_mixins(node: Node<'_>, mixins: &[Node<'_>]) -> String {
    let mixin_names: Vec<&str> = mixins
        .iter()
        .rev()
        .flat_map(|m| m.arguments().into_iter().map(|a| a.source()))
        .collect();
    format!(
        "{} {}",
        node.method_name().unwrap_or(""),
        mixin_names.join(", ")
    )
}
