//! Port of `lib/rubocop/cop/lint/incompatible_io_select_with_fiber_scheduler.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static IO_SELECT: NodePattern = NodePattern::new("(send (const {nil? cbase} :IO) :select $...)");

#[derive(Default)]
pub struct IncompatibleIoSelectWithFiberScheduler;

impl Cop for IncompatibleIoSelectWithFiberScheduler {
    fn name(&self) -> &'static str {
        "Lint/IncompatibleIoSelectWithFiberScheduler"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["select"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let args = IO_SELECT
            .match_node(node)
            .map(|c| c[0].nodes())
            .unwrap_or_default();
        let (read, write, excepts, timeout) =
            (args.first().copied(), args.get(1).copied(), args.get(2).copied(), args.get(3).copied());
        if excepts.is_some_and(|e| !e.children().is_empty()) {
            return;
        }
        if !(scheduler_compatible(read, write) || scheduler_compatible(write, read)) {
            return;
        }
        let preferred = preferred_method(read.unwrap(), write, timeout);
        let message = format!("Use `{preferred}` instead of `{}`.", node.source());
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if node.parent().is_some_and(|p| p.is_assignment()) {
                return;
            }
            corrector.replace(node.source_range(), &preferred);
        });
    }
}

fn scheduler_compatible(io1: Option<Node<'_>>, io2: Option<Node<'_>>) -> bool {
    if !single_io_array(io1) {
        return false;
    }
    match io2 {
        Some(io2) if io2.is(NodeType::Array) => io2.children().is_empty(),
        Some(io2) => io2.is(NodeType::Nil),
        None => true,
    }
}

fn single_io_array(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|node| {
        node.is(NodeType::Array)
            && node.children().len() == 1
            && !node.child(0).is_some_and(|v| v.is(NodeType::Splat))
    })
}

fn preferred_method(read: Node<'_>, write: Option<Node<'_>>, timeout: Option<Node<'_>>) -> String {
    let timeout_argument = timeout.map_or_else(String::new, |t| format!("({})", t.source()));
    if read.is(NodeType::Array)
        && let Some(value) = read.child(0)
    {
        format!("{}.wait_readable{timeout_argument}", value.source())
    } else {
        format!(
            "{}.wait_writable{timeout_argument}",
            write.and_then(|w| w.child(0)).unwrap().source()
        )
    }
}
