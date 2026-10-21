//! Port of `lib/rubocop/cop/style/redundant_regexp_constructor.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static REDUNDANT_REGEXP_CONSTRUCTOR: NodePattern = NodePattern::new(
    "(send (const {nil? cbase} :Regexp) {:new :compile} $(regexp _* (regopt _*)))",
);

#[derive(Default)]
pub struct RedundantRegexpConstructor;

impl Cop for RedundantRegexpConstructor {
    fn name(&self) -> &'static str {
        "Style/RedundantRegexpConstructor"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new", "compile"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = REDUNDANT_REGEXP_CONSTRUCTOR.match_node(node) else {
            return;
        };
        let regexp = captures[0].node().unwrap();
        let message = format!(
            "Remove the redundant `Regexp.{}`.",
            node.method_name().unwrap_or("")
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), regexp.source());
        });
    }
}
