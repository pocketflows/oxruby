//! Port of `lib/rubocop/cop/style/redundant_min_max_by.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static REDUNDANT_MINMAX_BY_BLOCK: NodePattern = NodePattern::new(
    "(block $(call _ {:max_by :min_by :minmax_by}) (args (arg $_x)) (lvar _x))",
);
static REDUNDANT_MINMAX_BY_NUMBLOCK: NodePattern =
    NodePattern::new("(numblock $(call _ {:max_by :min_by :minmax_by}) 1 (lvar :_1))");
static REDUNDANT_MINMAX_BY_ITBLOCK: NodePattern =
    NodePattern::new("(itblock $(call _ {:max_by :min_by :minmax_by}) _ (lvar :it))");

fn replacement(method: &str) -> &'static str {
    match method {
        "max_by" => "max",
        "min_by" => "min",
        "minmax_by" => "minmax",
        _ => "",
    }
}

#[derive(Default)]
pub struct RedundantMinMaxBy;

impl Cop for RedundantMinMaxBy {
    fn name(&self) -> &'static str {
        "Style/RedundantMinMaxBy"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let pattern = match node.ty() {
            NodeType::Block => &REDUNDANT_MINMAX_BY_BLOCK,
            NodeType::Numblock => &REDUNDANT_MINMAX_BY_NUMBLOCK,
            _ => &REDUNDANT_MINMAX_BY_ITBLOCK,
        };
        let Some(c) = pattern.match_node(node) else {
            return;
        };
        let send = c[0].node().unwrap();
        let method = send.method_name().unwrap_or("");
        let replacement = replacement(method);
        let message = match node.ty() {
            NodeType::Block => {
                let var = c[1].sym().unwrap_or("");
                format!("Use `{replacement}` instead of `{method} {{ |{var}| {var} }}`.")
            }
            NodeType::Numblock => format!("Use `{replacement}` instead of `{method} {{ _1 }}`."),
            _ => format!("Use `{replacement}` instead of `{method} {{ it }}`."),
        };
        let range = range_between(
            send.loc().selector.unwrap().begin_pos,
            node.loc().end.unwrap().end_pos,
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, replacement);
        });
    }
}
