//! Port of `lib/rubocop/cop/style/trailing_method_end_statement.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Place the end statement of a multi-line method on its own line.";

#[derive(Default)]
pub struct TrailingMethodEndStatement;

impl Cop for TrailingMethodEndStatement {
    fn name(&self) -> &'static str {
        "Style/TrailingMethodEndStatement"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_endless() || !trailing_end(node) {
            return;
        }
        let end = node.loc().end.unwrap();
        let column = node.column_of(node.loc().keyword.unwrap().begin_pos);
        cx.add_offense_with_correction(end, MSG, |corrector| {
            corrector.insert_before(end, format!("\n{}", " ".repeat(column)));
        });
    }
}

fn trailing_end(node: Node<'_>) -> bool {
    node.body().is_some() && node.is_multiline() && body_and_end_on_same_line(node)
}

fn body_and_end_on_same_line(node: Node<'_>) -> bool {
    let last_child = node.child_from_end(1).unwrap();
    last_child.last_line() == node.last_line_of(node.loc().end.unwrap())
}
