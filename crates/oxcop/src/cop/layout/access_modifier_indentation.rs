//! Port of `lib/rubocop/cop/layout/access_modifier_indentation.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::column_offset_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct AccessModifierIndentation;

impl Cop for AccessModifierIndentation {
    fn name(&self) -> &'static str {
        "Layout/AccessModifierIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[Class, Sclass, Module, Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = node.body().filter(|b| b.is(NodeType::Begin)) else {
            return;
        };
        check_body(cx, body, node);
    }
}

fn check_body<'a>(cx: &mut Investigation<'a, '_>, body: Node<'a>, node: Node<'a>) {
    let Some(end_range) = node.loc().end else {
        return;
    };
    let modifiers: Vec<Node<'a>> = body
        .child_nodes()
        .filter(|c| c.is(NodeType::Send) && c.is_bare_access_modifier())
        .collect();
    for modifier in modifiers {
        if node.first_line() == modifier.first_line() {
            continue;
        }
        check_modifier(cx, modifier, end_range);
    }
}

fn check_modifier<'a>(cx: &mut Investigation<'a, '_>, send_node: Node<'a>, end_range: Range) {
    let offset = column_offset_between(cx.processed_source, send_node.source_range(), end_range);
    let column_delta = expected_indent_offset(cx) as isize - offset;
    if column_delta == 0 {
        return;
    }
    let style = style(cx);
    let mut capitalized = style.to_owned();
    if let Some(first) = capitalized.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let message = format!("{capitalized} access modifiers like `{}`.", send_node.source());
    cx.add_offense_cx(send_node.source_range(), &message, |corrector, cx| {
        alignment_corrector::correct(
            corrector,
            cx.processed_source,
            cx.config,
            Some(Target::Node(send_node)),
            column_delta,
            false,
        );
    });
}

fn expected_indent_offset(cx: &Investigation<'_, '_>) -> usize {
    if style(cx) == "outdent" {
        0
    } else {
        configured_indentation_width(cx)
    }
}
