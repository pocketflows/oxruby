//! Port of `lib/rubocop/cop/style/redundant_fetch_block.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::frozen_string_literal::frozen_string_literals_enabled;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static RAILS_CACHE: NodePattern = NodePattern::new("(send (const _ :Rails) :cache)");

#[derive(Default)]
pub struct RedundantFetchBlock;

impl Cop for RedundantFetchBlock {
    fn name(&self) -> &'static str {
        "Style/RedundantFetchBlock"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // (block $(call _ :fetch _) (args) ${nil? basic_literal? const_type?})
        let Some(send) = node.send_node() else {
            return;
        };
        if !(send.is_call() && send.is_method("fetch") && send.children().len() == 3) {
            return;
        }
        if !node
            .child(1)
            .is_some_and(|a| a.is(NodeType::Args) && a.children().is_empty())
        {
            return;
        }
        let body = node.child(2);
        if body.is_some_and(|b| !(b.is_basic_literal() || b.is(NodeType::Const))) {
            return;
        }
        if should_not_check(cx, send, body) {
            return;
        }
        let range = range_between(
            send.loc().selector.unwrap().begin_pos,
            node.loc().end.unwrap().end_pos,
        );
        let key = send.child(2).unwrap().source();
        let default_value = body.map_or("nil", |b| b.source());
        let good = format!("fetch({key}, {default_value})");
        let block = body.map_or("{}".to_owned(), |b| format!("{{ {} }}", b.source()));
        let bad = format!("fetch({key}) {block}");
        let message = format!("Use `{good}` instead of `{bad}`.");
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, &good);
        });
    }
}

fn should_not_check(cx: &Investigation<'_, '_>, send: Node<'_>, body: Option<Node<'_>>) -> bool {
    (body.is_some_and(|b| b.is(NodeType::Const)) && !cx.cop_config_bool("SafeForConstants", false))
        || (body.is_some_and(|b| b.is(NodeType::Str)) && !frozen_string_literals_enabled(cx))
        || send.receiver().is_some_and(|r| RAILS_CACHE.is_match(r))
}
