//! Port of `lib/rubocop/cop/style/mixin_usage.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static INCLUDE_STATEMENT: NodePattern = NodePattern::new(
    "(send nil? ${:include :extend :prepend}
      const+)",
);
static IN_TOP_LEVEL_SCOPE: NodePattern = NodePattern::new(
    "{
      root?                        # either at the top level
      ^[  {kwbegin begin if def}   # or wrapped within one of these
          #in_top_level_scope? ]   # that is in top level scope
    }",
);

fn in_top_level_scope(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "in_top_level_scope?" && val.node().is_some_and(in_top_level_scope)
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    IN_TOP_LEVEL_SCOPE.match_with(node, &ctx).is_some()
}

#[derive(Default)]
pub struct MixinUsage;

impl Cop for MixinUsage {
    fn name(&self) -> &'static str {
        "Style/MixinUsage"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["include", "extend", "prepend"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(statement) = INCLUDE_STATEMENT.match_node(node).and_then(|c| c[0].sym()) else {
            return;
        };
        if !in_top_level_scope(node) {
            return;
        }
        let message =
            format!("`{statement}` is used at the top level. Use inside `class` or `module`.");
        cx.add_offense(node.source_range(), &message);
    }
}
