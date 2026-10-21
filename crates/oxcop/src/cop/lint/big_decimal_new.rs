//! Port of `lib/rubocop/cop/lint/big_decimal_new.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "`BigDecimal.new()` is deprecated. Use `BigDecimal()` instead.";

static BIG_DECIMAL_NEW: NodePattern =
    NodePattern::new("(send (const ${nil? cbase} :BigDecimal) :new ...)");

#[derive(Default)]
pub struct BigDecimalNew;

impl Cop for BigDecimalNew {
    fn name(&self) -> &'static str {
        "Lint/BigDecimalNew"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = BIG_DECIMAL_NEW.match_node(node) else {
            return;
        };
        let cbase = captures[0].node();
        let selector = node.loc().selector.unwrap();
        let dot = node.loc().dot;
        cx.add_offense_with_correction(selector, MSG, |corrector| {
            corrector.remove(selector);
            if let Some(dot) = dot {
                corrector.remove(dot);
            }
            if let Some(cbase) = cbase {
                corrector.remove(cbase.source_range());
            }
        });
    }
}
