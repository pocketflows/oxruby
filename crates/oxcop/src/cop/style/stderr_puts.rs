//! Port of `lib/rubocop/cop/style/stderr_puts.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static STDERR_PUTS: NodePattern = NodePattern::new(
    "(send
       {(gvar _) (const {nil? cbase} :STDERR)}
       :puts $_
       ...)",
);

#[derive(Default)]
pub struct StderrPuts;

impl Cop for StderrPuts {
    fn name(&self) -> &'static str {
        "Style/StderrPuts"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["puts"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !STDERR_PUTS.is_match(node) || !stderr_gvar(node.receiver().unwrap()) {
            return;
        }
        let message = format!(
            "Use `warn` instead of `{}.{}` to allow such output to be disabled.",
            node.receiver().unwrap().source(),
            node.method_name().unwrap()
        );
        let range = range_between(
            node.source_range().begin_pos,
            node.loc().selector.unwrap().end_pos,
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, "warn");
        });
    }
}

/// `(gvar #stderr_gvar?)`; constants pass.
fn stderr_gvar(receiver: Node<'_>) -> bool {
    !receiver.is(NodeType::Gvar) || receiver.name() == Some("$stderr")
}
