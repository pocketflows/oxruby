//! Port of `lib/rubocop/cop/layout/end_alignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::check_assignment;
use crate::cop::mixin::end_keyword_alignment::{
    check_end_kw_alignment, line_break_before_keyword, start_line_range, style,
};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::first_part_of_call_chain;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct EndAlignment {
    ignored: IgnoredNodes,
}

impl Cop for EndAlignment {
    fn name(&self) -> &'static str {
        "Layout/EndAlignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            Class, Sclass, Module, If, While, Until, Case, CaseMatch, Lvasgn, Ivasgn, Cvasgn,
            Gvasgn, Casgn, Masgn, OpAsgn, OrAsgn, AndAsgn, Send,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored = IgnoredNodes::default();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        match node.ty() {
            Class | Module | While | Until => self.check_other_alignment(cx, node),
            Sclass => match node.parent().filter(|p| assignment(*p)) {
                Some(parent) => self.check_asgn_alignment(cx, parent, node),
                None => self.check_other_alignment(cx, node),
            },
            If => {
                if !node.is_ternary() {
                    self.check_other_alignment(cx, node);
                }
            }
            Case | CaseMatch => {
                if node.is_argument() {
                    self.check_asgn_alignment(cx, node.parent().unwrap(), node);
                } else {
                    self.check_other_alignment(cx, node);
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

impl EndAlignment {
    fn check_assignment<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        rhs: Option<Node<'a>>,
    ) {
        let Some(mut rhs) = rhs.and_then(first_part_of_call_chain) else {
            return;
        };
        while rhs.is_any(&[NodeType::Begin, NodeType::Or, NodeType::And]) {
            match rhs.child_nodes().next() {
                Some(child) => rhs = child,
                None => return,
            }
        }
        if !rhs.is_conditional() {
            return;
        }
        if rhs.is(NodeType::If) && rhs.is_ternary() {
            return;
        }
        self.check_asgn_alignment(cx, node, rhs);
    }

    fn check_asgn_alignment<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        outer_node: Node<'a>,
        inner_node: Node<'a>,
    ) {
        let ps = cx.processed_source;
        let Some(keyword) = inner_node.loc().keyword else {
            return;
        };
        let align_with = [
            ("keyword", keyword),
            ("start_of_line", start_line_range(ps, inner_node)),
            ("variable", asgn_variable_align_with(cx, outer_node, inner_node)),
        ];
        check_end_kw_alignment(cx, &self.ignored, inner_node, &align_with, |corrector, cx| {
            autocorrect(corrector, cx, inner_node);
        });
        self.ignored.ignore(inner_node);
    }

    fn check_other_alignment<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ps = cx.processed_source;
        let Some(keyword) = node.loc().keyword else {
            return;
        };
        let align_with = [
            ("keyword", keyword),
            ("variable", keyword),
            ("start_of_line", start_line_range(ps, node)),
        ];
        check_end_kw_alignment(cx, &self.ignored, node, &align_with, |corrector, cx| {
            autocorrect(corrector, cx, node);
        });
    }
}

fn asgn_variable_align_with(
    cx: &Investigation<'_, '_>,
    outer_node: Node<'_>,
    inner_node: Node<'_>,
) -> crate::processed_source::Range {
    let expr = outer_node.source_range();
    let keyword = inner_node.loc().keyword.unwrap();
    if line_break_before_keyword(cx.processed_source, expr, inner_node) {
        keyword
    } else {
        range_between(expr.begin_pos, keyword.end_pos)
    }
}

fn autocorrect(
    corrector: &mut crate::cop::Corrector,
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
) {
    let align_to = alignment_node(cx, node);
    alignment_corrector::align_end(corrector, cx.processed_source, cx.config, node, Some(align_to));
}

fn alignment_node<'a>(cx: &Investigation<'_, '_>, node: Node<'a>) -> Target<'a> {
    match style(cx) {
        "keyword" => Target::Node(node),
        "variable" => {
            let mut align_to = alignment_node_for_variable_style(cx, node);
            while let Some(parent) = align_to.parent() {
                if !(parent.is_send() && align_to.first_line() == parent.first_line()) {
                    break;
                }
                align_to = parent;
            }
            Target::Node(align_to)
        }
        _ => Target::Range(start_line_range(cx.processed_source, node)),
    }
}

fn alignment_node_for_variable_style<'a>(cx: &Investigation<'_, '_>, node: Node<'a>) -> Node<'a> {
    if node.is_any(&[NodeType::Case, NodeType::CaseMatch])
        && node.is_argument()
        && node.parent().is_some_and(|p| p.first_line() == node.first_line())
    {
        return node.parent().unwrap();
    }
    match assignment_or_operator_method(node) {
        Some(assignment)
            if !line_break_before_keyword(cx.processed_source, assignment.source_range(), node) =>
        {
            assignment
        }
        _ => node,
    }
}

fn assignment_or_operator_method(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors().find(|ancestor| {
        assignment_or_similar(*ancestor) || (ancestor.is_send() && ancestor.is_operator_method())
    })
}

/// `assignment?`: on a call (`MethodDispatchNode`) it is `setter_method?`.
fn assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

/// `assignment_or_similar?`: `{assignment? (send _recv :<< ...)}`.
fn assignment_or_similar(node: Node<'_>) -> bool {
    assignment(node) || (node.is_send() && node.is_method("<<"))
}
