//! Port of `lib/rubocop/cop/lint/refinement_import_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct RefinementImportMethods;

impl Cop for RefinementImportMethods {
    fn name(&self) -> &'static str {
        "Lint/RefinementImportMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.1)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["include", "prepend"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_some() {
            return;
        }
        let Some(parent) = node.parent() else {
            return;
        };
        if !(parent.is(NodeType::Block) && parent.is_method("refine")) {
            return;
        }

        let current = node.method_name().unwrap_or_default();
        let message = if cx.target_ruby_version() >= 3.2 {
            format!(
                "Use `import_methods` instead of `{current}` because it was removed in Ruby 3.2."
            )
        } else {
            format!(
                "Use `import_methods` instead of `{current}` because it is deprecated in Ruby 3.1."
            )
        };
        let Some(selector) = node.loc().selector else {
            return;
        };
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.replace(selector, "import_methods");
        });
    }
}
