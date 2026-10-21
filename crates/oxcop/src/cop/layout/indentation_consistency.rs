//! Port of `lib/rubocop/cop/layout/indentation_consistency.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::{check_alignment, display_column};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "Inconsistent indentation detected.";

#[derive(Default)]
pub struct IndentationConsistency;

impl Cop for IndentationConsistency {
    fn name(&self) -> &'static str {
        "Layout/IndentationConsistency"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Begin, NodeType::Kwbegin]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if style(cx) == "indented_internal_methods" {
            check_indented_internal_methods_style(cx, node);
        } else {
            check_normal_style(cx, node);
        }
    }
}

fn autocorrect<'a>(corrector: &mut Corrector, cx: &Investigation<'a, '_>, node: Option<Node<'a>>, column_delta: isize) {
    alignment_corrector::correct(
        corrector,
        cx.processed_source,
        cx.config,
        node.map(Target::Node),
        column_delta,
        false,
    );
}

fn bare_access_modifier(node: Node<'_>) -> bool {
    node.is(NodeType::Send) && node.is_bare_access_modifier()
}

fn base_column_for_normal_style(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<usize> {
    let first_child = node.child(0).filter(|c| bare_access_modifier(*c))?;
    let ps = cx.processed_source;
    let access_modifier_indent = display_column(ps, first_child.source_range());
    let Some(parent) = node.parent() else {
        return Some(access_modifier_indent);
    };
    let module_indent = display_column(ps, parent.source_range());
    (access_modifier_indent > module_indent).then_some(access_modifier_indent)
}

fn check_normal_style<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let children: Vec<Node<'a>> = node
        .child_nodes()
        .filter(|child| !bare_access_modifier(*child))
        .collect();
    let base = base_column_for_normal_style(cx, node);
    check_alignment(cx, &children, base, &|_, _| MSG.to_owned(), &autocorrect);
}

fn check_indented_internal_methods_style<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let mut children_to_check: Vec<Vec<Node<'a>>> = vec![Vec::new()];
    for child in node.child_nodes() {
        if bare_access_modifier(child) {
            children_to_check.push(Vec::new());
        } else {
            children_to_check.last_mut().unwrap().push(child);
        }
    }
    for group in children_to_check {
        check_alignment(cx, &group, None, &|_, _| MSG.to_owned(), &autocorrect);
    }
}
