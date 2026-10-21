//! Port of `lib/rubocop/cop/layout/def_end_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::end_keyword_alignment::{check_end_kw_alignment, check_end_kw_in_node, style};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};

#[derive(Default)]
pub struct DefEndAlignment {
    ignored: IgnoredNodes,
}

impl Cop for DefEndAlignment {
    fn name(&self) -> &'static str {
        "Layout/DefEndAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs, NodeType::Send]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_def() {
            check_end_kw_in_node(cx, &self.ignored, node, |corrector, cx| autocorrect(corrector, cx, node));
            return;
        }
        if !node.is_def_modifier() {
            return;
        }
        let Some(method_def) = node.descendants().find(|d| d.is_any_def()) else {
            return;
        };
        let expr = node.source_range();
        let keyword = method_def.loc().keyword.unwrap();
        let line_start = range_between(expr.begin_pos, keyword.end_pos);
        let align_with = [("def", keyword), ("start_of_line", line_start)];
        check_end_kw_alignment(cx, &self.ignored, method_def, &align_with, |corrector, cx| {
            autocorrect(corrector, cx, method_def);
        });
        self.ignored.ignore(method_def);
    }
}

fn autocorrect(corrector: &mut Corrector, cx: &Investigation<'_, '_>, node: Node<'_>) {
    let align_to = match node.parent() {
        Some(parent) if style(cx) == "start_of_line" && parent.is_send() => parent,
        _ => node,
    };
    alignment_corrector::align_end(corrector, cx.processed_source, cx.config, node, Some(Target::Node(align_to)));
}
