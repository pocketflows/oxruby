//! Port of `lib/rubocop/cop/gemspec/add_runtime_dependency.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `add_dependency` instead of `add_runtime_dependency`.";

#[derive(Default)]
pub struct AddRuntimeDependency;

impl Cop for AddRuntimeDependency {
    fn name(&self) -> &'static str {
        "Gemspec/AddRuntimeDependency"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["add_runtime_dependency"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_none() || node.arguments().is_empty() {
            return;
        }
        let selector = node.loc().selector.unwrap();
        cx.add_offense_with_correction(selector, MSG, |corrector| {
            corrector.replace(selector, "add_dependency");
        });
    }
}
