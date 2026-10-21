//! Port of `lib/rubocop/cop/style/redundant_capital_w.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::percent_literal::process;
use crate::cop::mixin::util::double_quotes_required;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not use `%W` unless interpolation is needed. If not, use `%w`.";

#[derive(Default)]
pub struct RedundantCapitalW;

impl Cop for RedundantCapitalW {
    fn name(&self) -> &'static str {
        "Style/RedundantCapitalW"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !process(node, &["%W"]) || requires_interpolation(node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let begin = node.loc().begin.unwrap();
            corrector.replace(begin, node.range_source(begin).replace('W', "w"));
        });
    }
}

fn requires_interpolation(node: Node<'_>) -> bool {
    node.child_nodes()
        .any(|string| string.is(NodeType::Dstr) || double_quotes_required(string.source()))
}
