//! Port of `lib/rubocop/cop/style/colon_method_call.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not use `::` for method calls.";

static JAVA_ROOT: NodePattern = NodePattern::new("(const nil? :Java)");

#[derive(Default)]
pub struct ColonMethodCall;

impl Cop for ColonMethodCall {
    fn name(&self) -> &'static str {
        "Style/ColonMethodCall"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/RedundantSelf"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_none() || !node.is_double_colon() {
            return;
        }
        if node.is_camel_case_method() || java_interop(node) {
            return;
        }
        let dot = node.loc().dot.unwrap();
        cx.add_offense_with_correction(dot, MSG, |corrector| corrector.replace(dot, "."));
    }
}

/// `receiver.respond_to?(:receiver)`: node classes with a `receiver`.
fn responds_to_receiver(node: Node<'_>) -> bool {
    node.is_call() || node.is_any_def() || node.is_any_block()
}

fn java_interop(node: Node<'_>) -> bool {
    let mut receiver = node.receiver().unwrap();
    while responds_to_receiver(receiver) {
        match receiver.receiver() {
            Some(r) => receiver = r,
            None => break,
        }
    }
    JAVA_ROOT.is_match(receiver)
}
