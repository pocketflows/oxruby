//! Port of `lib/rubocop/cop/layout/else_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::check_assignment;
use crate::cop::mixin::end_keyword_alignment::{start_line_range, variable_alignment};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::column_offset_between;
use crate::cop::mixin::util::{begins_its_line, first_part_of_call_chain};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

#[derive(Default)]
pub struct ElseAlignment {
    ignored: IgnoredNodes,
}

impl Cop for ElseAlignment {
    fn name(&self) -> &'static str {
        "Layout/ElseAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            If, Rescue, Case, CaseMatch, Lvasgn, Ivasgn, Cvasgn, Gvasgn, Casgn, Masgn, OpAsgn,
            OrAsgn, AndAsgn, Send,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::If => self.on_if(cx, node, None),
            NodeType::Rescue => {
                if let Some(else_range) = node.loc().else_ {
                    let base = base_range_of_rescue(cx, node);
                    check_alignment(cx, base, else_range);
                }
            }
            NodeType::Case | NodeType::CaseMatch => {
                if !node.has_else() {
                    return;
                }
                let last = if node.is(NodeType::Case) {
                    node.when_branches().last().copied()
                } else {
                    node.child_nodes()
                        .rfind(|c| c.is(NodeType::InPattern))
                };
                if let Some(keyword) = last.and_then(|l| l.loc().keyword) {
                    check_alignment(cx, keyword, node.loc().else_.unwrap());
                }
            }
            _ => {
                if let Some((node, rhs)) = check_assignment::assignment(node) {
                    self.check_assignment(cx, node, rhs);
                }
            }
        }
    }
}

impl ElseAlignment {
    fn on_if<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, base: Option<Node<'a>>) {
        if self.ignored.contains(node) {
            return;
        }
        let Some(else_range) = node.loc().else_ else {
            return;
        };
        if !begins_its_line(cx.processed_source, else_range) {
            return;
        }
        let Some(base_range) = base_range_of_if(node, base) else {
            return;
        };
        check_alignment(cx, base_range, else_range);
        if !node.is_elsif_conditional() {
            return;
        }
        if let Some(else_branch) = node.else_branch() {
            self.check_nested(cx, else_branch, base);
        }
    }

    fn check_nested<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, base: Option<Node<'a>>) {
        self.on_if(cx, node, base);
        self.ignored.ignore(node);
    }

    fn check_assignment<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>, rhs: Option<Node<'a>>) {
        let Some(rhs) = rhs.and_then(first_part_of_call_chain) else {
            return;
        };
        let style = cx
            .config
            .for_cop("Layout/EndAlignment")
            .get("EnforcedStyleAlignWith")
            .and_then(serde_yaml_ng::Value::as_str)
            .unwrap_or("keyword");
        let base = if variable_alignment(cx.processed_source, node.source_range(), rhs, style) {
            node
        } else {
            rhs
        };
        if !rhs.is(NodeType::If) {
            return;
        }
        self.check_nested(cx, rhs, Some(base));
    }
}

fn base_range_of_if(node: Node<'_>, base: Option<Node<'_>>) -> Option<Range> {
    if let Some(base) = base {
        return Some(base.source_range());
    }
    std::iter::once(node)
        .chain(node.ancestors().filter(|a| a.is(NodeType::If)))
        .find(|parent| parent.is_if() || parent.is_unless())
        .and_then(|parent| parent.loc().keyword)
}

fn base_range_of_rescue(cx: &Investigation<'_, '_>, node: Node<'_>) -> Range {
    let Some(mut parent) = node.parent() else {
        return node.loc().keyword.unwrap_or(node.source_range());
    };
    if parent.is(NodeType::Ensure)
        && let Some(grandparent) = parent.parent()
    {
        parent = grandparent;
    }
    use NodeType::*;
    match parent.ty() {
        Def | Defs => base_for_method_definition(parent),
        Kwbegin => parent.loc().begin.unwrap(),
        Block | Numblock | Itblock => start_line_range(cx.processed_source, parent),
        Class | Module | Sclass => parent.loc().keyword.unwrap(),
        _ => node.loc().keyword.unwrap_or(node.source_range()),
    }
}

fn base_for_method_definition(node: Node<'_>) -> Range {
    match node.parent() {
        Some(parent) if parent.is_send() => parent.loc().selector.unwrap(),
        _ => node.loc().keyword.unwrap(),
    }
}

fn check_alignment(cx: &mut Investigation<'_, '_>, base_range: Range, else_range: Range) {
    let ps = cx.processed_source;
    if !begins_its_line(ps, else_range) {
        return;
    }
    let column_delta = column_offset_between(ps, base_range, else_range);
    if column_delta == 0 {
        return;
    }
    let base_source = ps.slice(base_range);
    let base_word = regex!(r"^\S*")
        .find(base_source)
        .map_or("", |(b, e)| &base_source[b..e]);
    let message = format!("Align `{}` with `{base_word}`.", ps.slice(else_range));
    cx.add_offense_cx(else_range, &message, |corrector, cx| {
        alignment_corrector::correct(
            corrector,
            cx.processed_source,
            cx.config,
            Some(Target::Range(else_range)),
            column_delta,
            false,
        );
    });
}
