//! Port of `lib/rubocop/cop/layout/begin_end_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::end_keyword_alignment::{check_end_kw_alignment, start_line_range, style};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct BeginEndAlignment {
    ignored: IgnoredNodes,
}

impl Cop for BeginEndAlignment {
    fn name(&self) -> &'static str {
        "Layout/BeginEndAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Kwbegin]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ps = cx.processed_source;
        let Some(begin) = node.loc().begin else {
            return;
        };
        let align_with = [("begin", begin), ("start_of_line", start_line_range(ps, node))];
        check_end_kw_alignment(cx, &self.ignored, node, &align_with, |corrector, cx| {
            let align_to = if style(cx) == "begin" {
                Target::Node(node)
            } else {
                Target::Range(start_line_range(cx.processed_source, node))
            };
            alignment_corrector::align_end(corrector, cx.processed_source, cx.config, node, Some(align_to));
        });
    }
}
