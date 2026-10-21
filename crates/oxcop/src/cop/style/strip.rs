//! Port of `lib/rubocop/cop/style/strip.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static LSTRIP_RSTRIP: NodePattern = NodePattern::new(
    "{
       (call $(call _ :rstrip) :lstrip)
       (call $(call _ :lstrip) :rstrip)
     }",
);

#[derive(Default)]
pub struct Strip;

impl Cop for Strip {
    fn name(&self) -> &'static str {
        "Style/Strip"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["lstrip", "rstrip"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(first_send) = LSTRIP_RSTRIP.match_node(node).and_then(|c| c[0].node()) else {
            return;
        };
        let range = range_between(
            first_send.loc().selector.unwrap().begin_pos,
            node.source_range().end_pos,
        );
        let message = format!("Use `strip` instead of `{}`.", cx.source(range));
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, "strip");
        });
    }
}
