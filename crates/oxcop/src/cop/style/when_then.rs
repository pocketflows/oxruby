//! Port of `lib/rubocop/cop/style/when_then.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct WhenThen;

impl Cop for WhenThen {
    fn name(&self) -> &'static str {
        "Style/WhenThen"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::When]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_multiline() || node.has_then() || node.body().is_none() {
            return;
        }
        let expression = node
            .conditions()
            .iter()
            .map(|c| c.source())
            .collect::<Vec<_>>()
            .join(", ");
        let message =
            format!("Do not use `when {expression};`. Use `when {expression} then` instead.");
        let Some(begin) = node.loc().begin else {
            return;
        };
        cx.add_offense_with_correction(begin, &message, |corrector| {
            corrector.replace(begin, " then");
        });
    }
}
