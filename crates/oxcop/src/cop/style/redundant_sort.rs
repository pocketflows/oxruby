//! Port of `lib/rubocop/cop/style/redundant_sort.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Corrector, Cop, Investigation};

static REDUNDANT_SORT: NodePattern = NodePattern::new(
    "{
      (call $(call _ $:sort) ${:last :first})
      (call $(call _ $:sort) ${:[] :at :slice} {(int 0) (int -1)})

      (call $(call _ $:sort_by _) ${:last :first})
      (send $(send _ $:sort_by _) ${:[] :at :slice} {(int 0) (int -1)})

      (call (any_block $(call _ ${:sort_by :sort}) ...) ${:last :first})
      (call
        (any_block $(call _ ${:sort_by :sort}) ...)
        ${:[] :at :slice} {(int 0) (int -1)}
      )
    }",
);

#[derive(Default)]
pub struct RedundantSort;

impl Cop for RedundantSort {
    fn name(&self) -> &'static str {
        "Style/RedundantSort"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["sort", "sort_by"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let parent = node.parent();
        let grandparent = parent.and_then(|p| p.parent());
        let Some((ancestor, sort_node, sorter, accessor)) = [parent, grandparent]
            .into_iter()
            .flatten()
            .find_map(|n| {
                let c = REDUNDANT_SORT.match_node(n)?;
                Some((n, c[0].node()?, c[1].sym()?, c[2].sym()?))
            })
        else {
            return;
        };
        let accessor_source = ancestor.range_source(range_between(
            ancestor.loc().selector.unwrap().begin_pos,
            ancestor.source_range().end_pos,
        ));
        let suggestion = suggestion(sorter, accessor, arg_value(ancestor));
        let message = format!("Use `{suggestion}` instead of `{sorter}...{accessor_source}`.");
        let range = range_between(
            sort_node.loc().selector.unwrap().begin_pos,
            ancestor.source_range().end_pos,
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            autocorrect(corrector, ancestor, sort_node, &suggestion);
        });
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, sort_node: Node<'_>, suggestion: &str) {
    corrector.remove(range_between(accessor_start(node), node.source_range().end_pos));
    corrector.replace(sort_node.loc().selector.unwrap(), suggestion);
    if let Some(parent) = node.parent().filter(|p| p.is_operator_keyword()) {
        let operator = parent.loc().operator.unwrap();
        corrector.insert_after(
            node.child_nodes().next().unwrap().source_range(),
            format!(" {}", parent.range_source(operator)),
        );
        corrector.remove(operator);
    }
}

fn suggestion(sorter: &str, accessor: &str, arg: Option<i64>) -> String {
    let base = if accessor == "first" || arg == Some(0) {
        "min"
    } else if accessor == "last" || arg == Some(-1) {
        "max"
    } else {
        ""
    };
    let suffix = if sorter == "sort_by" { "_by" } else { "" };
    format!("{base}{suffix}")
}

fn arg_value(node: Node<'_>) -> Option<i64> {
    node.first_argument().and_then(|a| a.int_value())
}

fn accessor_start(node: Node<'_>) -> usize {
    match node.loc().dot {
        Some(dot) => dot.begin_pos,
        None => node.loc().selector.unwrap().begin_pos,
    }
}
