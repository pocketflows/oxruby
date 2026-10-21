//! Port of `lib/rubocop/cop/lint/to_json.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "`#to_json` requires an optional argument to be parsable via JSON.generate(obj).";

#[derive(Default)]
pub struct ToJson;

impl Cop for ToJson {
    fn name(&self) -> &'static str {
        "Lint/ToJSON"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_method("to_json") && node.arguments().is_empty()) {
            return;
        }

        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            let opening_parenthesis = node
                .child_nodes()
                .find(|c| c.is(NodeType::Args))
                .and_then(|args| args.loc().begin);
            if let Some(opening_parenthesis) = opening_parenthesis {
                corrector.insert_after(opening_parenthesis, "*_args");
            } else if let Some(name) = node.loc().name {
                corrector.insert_after(name, "(*_args)");
            }
        });
    }
}
