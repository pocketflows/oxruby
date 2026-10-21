//! Port of `lib/rubocop/cop/style/multiline_memoization.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Corrector, Investigation};

const KEYWORD_MSG: &str = "Wrap multiline memoization blocks in `begin` and `end`.";
const BRACES_MSG: &str = "Wrap multiline memoization blocks in `(` and `)`.";

#[derive(Default)]
pub struct MultilineMemoization;

impl Cop for MultilineMemoization {
    fn name(&self) -> &'static str {
        "Style/MultilineMemoization"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::OrAsgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(rhs) = node.expression() else {
            return;
        };
        let keyword = style(cx) == "keyword";
        if !bad_rhs(keyword, rhs) {
            return;
        }
        let message = if style(cx) == "braces" {
            BRACES_MSG
        } else {
            KEYWORD_MSG
        };
        cx.add_offense_cx(node.source_range(), message, |corrector, cx| {
            if keyword {
                keyword_autocorrect(cx, rhs, corrector);
            } else {
                corrector.replace(rhs.loc().begin.unwrap(), "(");
                corrector.replace(rhs.loc().end.unwrap(), ")");
            }
        });
    }
}

fn bad_rhs(keyword: bool, rhs: Node<'_>) -> bool {
    if !rhs.is_multiline() {
        return false;
    }
    if keyword {
        rhs.is(NodeType::Begin)
    } else {
        // A `begin` block with `rescue`/`ensure` cannot be expressed with
        // parentheses.
        rhs.is(NodeType::Kwbegin)
            && !rhs
                .child_nodes()
                .any(|c| c.is_any(&[NodeType::Rescue, NodeType::Ensure]))
    }
}

fn keyword_autocorrect(cx: &Investigation<'_, '_>, node: Node<'_>, corrector: &mut Corrector) {
    let ps = cx.processed_source;
    let source = ps.buffer_source();
    let begin = node.loc().begin.unwrap();
    let end = node.loc().end.unwrap();
    let begin_str = if source.as_bytes().get(begin.end_pos) == Some(&b'\n') {
        "begin".to_owned()
    } else {
        format!(
            "begin\n{}",
            " ".repeat(node.column() + configured_indentation_width(cx))
        )
    };
    let end_line = ps.source_line(ps.line_of(end.begin_pos));
    let end_str = if crate::regex!(r"[^\s)]").find(end_line).is_some() {
        format!("\n{}end", " ".repeat(node.column()))
    } else {
        "end".to_owned()
    };
    corrector.replace(begin, begin_str);
    corrector.replace(end, end_str);
}
