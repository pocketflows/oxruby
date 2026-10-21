//! Port of `lib/rubocop/cop/gemspec/ruby_version_globals_usage.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::gemspec_help::gem_specification;
use crate::cop::{Cop, Investigation};

static RUBY_VERSION: NodePattern = NodePattern::new(
    "{
       (const {cbase nil?} :RUBY_VERSION)
       (const (const {cbase nil?} :Ruby) :VERSION)
     }",
);

#[derive(Default)]
pub struct RubyVersionGlobalsUsage;

impl Cop for RubyVersionGlobalsUsage {
    fn name(&self) -> &'static str {
        "Gemspec/RubyVersionGlobalsUsage"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if gem_specification(cx.processed_source.ast()).is_none() || !RUBY_VERSION.is_match(node) {
            return;
        }
        let message = format!("Do not use `{}` in gemspec file.", node.source());
        cx.add_offense(node.source_range(), &message);
    }
}
