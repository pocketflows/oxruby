//! Port of `lib/rubocop/cop/layout/assignment_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{check_alignment, configured_indentation_width, display_column};
use crate::cop::mixin::check_assignment;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Indent the first line of the right-hand-side of a multi-line assignment.";

#[derive(Default)]
pub struct AssignmentIndentation;

impl Cop for AssignmentIndentation {
    fn name(&self) -> &'static str {
        "Layout/AssignmentIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        check_assignment::NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some((node, rhs)) = check_assignment::assignment(node) {
            check(cx, node, rhs);
        }
    }
}

fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, rhs: Option<Node<'a>>) {
    let Some(rhs) = rhs else {
        return;
    };
    let Some(operator) = node.loc().operator else {
        return;
    };
    if node.line_of(operator) == rhs.first_line() {
        return;
    }
    let base = display_column(
        cx.processed_source,
        leftmost_multiple_assignment(node).source_range(),
    );
    let base = base + configured_indentation_width(cx);
    check_alignment(
        cx,
        &[rhs],
        Some(base),
        &|_, _| MSG.to_owned(),
        &|corrector, cx, node, delta| {
            alignment_corrector::correct(
                corrector,
                cx.processed_source,
                cx.config,
                node.map(Target::Node),
                delta,
                false,
            );
        },
    );
}

/// `assignment?`: on a call (`MethodDispatchNode`) it is `setter_method?`.
fn assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

fn leftmost_multiple_assignment(node: Node<'_>) -> Node<'_> {
    match node.parent() {
        Some(parent) if parent.first_line() == node.first_line() && assignment(parent) => {
            // The recursive call's result is discarded upstream.
            parent
        }
        _ => node,
    }
}
