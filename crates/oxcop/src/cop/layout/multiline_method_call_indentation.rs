//! Port of `lib/rubocop/cop/layout/multiline_method_call_indentation.rb`.

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::correctors::alignment_corrector::{self, Target};
use crate::cop::mixin::alignment::configured_indentation_width;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::multiline_expression_indentation::{
    self as mei, ArgumentKind, MultilineExpressionIndentation as Mei, argument_in_method_call,
    assignment_rhs, correct_indentation, grouped_expression, indentation,
    indented_keyword_expression, inside_arg_list_parentheses, kw_node_with_special_indentation,
    not_for_this_cop, operation_description, part_of_assignment_rhs, within_node,
};
use crate::cop::mixin::range_help::{range_between, range_by_whole_lines};
use crate::cop::mixin::util::{begins_its_line, same_line};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

#[derive(Default)]
pub struct MultilineMethodCallIndentation {
    column_delta: isize,
    send_node: Option<NodeId>,
    base: Option<Range>,
    hash_pair_base_column: Option<usize>,
}

impl Cop for MultilineMethodCallIndentation {
    fn name(&self) -> &'static str {
        "Layout/MultilineMethodCallIndentation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        mei::on_send(self, cx, node);
    }
}

fn column(ps: &ProcessedSource<'_>, range: Range) -> usize {
    ps.column_of(range.begin_pos)
}

fn line(ps: &ProcessedSource<'_>, range: Range) -> usize {
    ps.line_of(range.begin_pos)
}

fn find_base_receiver(node: Node<'_>) -> Node<'_> {
    let mut base_receiver = node;
    while let Some(receiver) = base_receiver.receiver() {
        base_receiver = receiver;
    }
    base_receiver
}

fn find_pair_ancestor(node: Node<'_>) -> Option<Node<'_>> {
    for ancestor in node.ancestors() {
        if ancestor.is(NodeType::Pair) {
            return Some(ancestor);
        }
        if grouped_expression(ancestor) || inside_arg_list_parentheses(node, ancestor) {
            break;
        }
    }
    None
}

fn unwrap_block_node(node: Option<Node<'_>>) -> Option<Node<'_>> {
    match node {
        Some(n) if n.is_any_block() => n.send_node(),
        other => other,
    }
}

/// `dot.join(selector)` for a call.
fn dot_selector(node: Node<'_>) -> Range {
    let loc = node.loc();
    let dot = loc.dot.unwrap();
    match loc.selector {
        Some(selector) => dot.join(selector),
        None => dot,
    }
}

impl MultilineMethodCallIndentation {
    fn send_node<'a>(&self, cx: &Investigation<'a, '_>) -> Node<'a> {
        cx.processed_source.tree().node(self.send_node.unwrap())
    }

    fn correct_selector_only(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, range: Range) {
        let ps = cx.processed_source;
        let selector_line = ps.line_range(ps.line_of(range.begin_pos));
        let selector_range = range_between(selector_line.begin_pos, selector_line.end_pos);
        alignment_corrector::correct(
            corrector,
            ps,
            cx.config,
            Some(Target::Range(selector_range)),
            self.column_delta,
            false,
        );
    }

    fn correct_block(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, block_node: Node<'_>) {
        let ps = cx.processed_source;
        alignment_corrector::correct(
            corrector,
            ps,
            cx.config,
            block_node.body().map(Target::Node),
            self.column_delta,
            false,
        );
        let end_range = range_by_whole_lines(ps, block_node.loc().end.unwrap(), false);
        alignment_corrector::correct(
            corrector,
            ps,
            cx.config,
            Some(Target::Range(end_range)),
            self.column_delta,
            false,
        );
    }

    fn skip_for_context(node: Node<'_>, pair_ancestor: Option<Node<'_>>) -> bool {
        if pair_ancestor.is_some() {
            inside_multiline_chain_arg(node)
        } else {
            not_for_this_cop(node)
        }
    }

    fn check_hash_pair_indented_style(
        &mut self,
        cx: &Investigation<'_, '_>,
        rhs: Range,
        pair_ancestor: Node<'_>,
    ) -> Option<Range> {
        let ps = cx.processed_source;
        let pair_key = pair_ancestor.key().unwrap();
        let double_indentation = configured_indentation_width(cx) * 2;
        let key_column = column(ps, pair_key.source_range());
        let correct_column = key_column + double_indentation;
        self.hash_pair_base_column = Some(key_column + configured_indentation_width(cx));
        self.calculate_column_delta_offense(ps, rhs, correct_column)
    }

    fn check_hash_pair_indentation(
        &mut self,
        cx: &Investigation<'_, '_>,
        node: Node<'_>,
        lhs: Node<'_>,
        rhs: Range,
    ) -> Option<Range> {
        let ps = cx.processed_source;
        self.base = find_hash_pair_alignment_base(node);
        if self.base.is_none() && inside_multiline_chain_arg(node) {
            return None;
        }
        if self.base.is_none() {
            self.base = Some(first_dot_alignment_base(ps, node, rhs).unwrap_or(lhs.source_range()));
        }
        if aligned_with_first_line_dot(ps, node, rhs) {
            return None;
        }
        let base_column = column(ps, self.base.unwrap());
        self.calculate_column_delta_offense(ps, rhs, base_column)
    }

    fn check_regular_indentation(
        &mut self,
        cx: &Investigation<'_, '_>,
        node: Node<'_>,
        lhs: Node<'_>,
        rhs: Range,
        given_style: &str,
    ) -> Option<Range> {
        let ps = cx.processed_source;
        self.base = alignment_base(ps, node, rhs, given_style);
        let correct_column = match self.base {
            Some(base) => {
                let mut parent = node.parent();
                if let Some(p) = parent
                    && p.is_any_block()
                {
                    parent = p.parent();
                }
                (column(ps, base) as isize + extra_indentation(cx, given_style, parent)) as usize
            }
            None => indentation(ps, lhs) + correct_indentation(cx, node),
        };
        self.calculate_column_delta_offense(ps, rhs, correct_column)
    }

    fn calculate_column_delta_offense(
        &mut self,
        ps: &ProcessedSource<'_>,
        rhs: Range,
        correct_column: usize,
    ) -> Option<Range> {
        self.column_delta = correct_column as isize - column(ps, rhs) as isize;
        (self.column_delta != 0).then_some(rhs)
    }

    fn should_indent_relative_to_receiver(&self, cx: &Investigation<'_, '_>) -> bool {
        self.base.is_some() && style(cx) == "indented_relative_to_receiver"
    }

    fn should_align_with_base(&self, cx: &Investigation<'_, '_>) -> bool {
        self.base.is_some() && style(cx) == "aligned"
    }

    fn base_source<'s>(&self, ps: &ProcessedSource<'s>) -> &'s str {
        let source = ps.slice(self.base.unwrap());
        source.split('\n').next().unwrap_or("")
    }

    fn relative_to_receiver_message(&self, cx: &Investigation<'_, '_>, rhs: Range) -> String {
        let ps = cx.processed_source;
        format!(
            "Indent `{}` {} spaces more than `{}` on line {}.",
            ps.slice(rhs),
            configured_indentation_width(cx),
            self.base_source(ps),
            line(ps, self.base.unwrap())
        )
    }

    fn align_with_base_message(&self, ps: &ProcessedSource<'_>, rhs: Range) -> String {
        format!(
            "Align `{}` with `{}` on line {}.",
            ps.slice(rhs),
            self.base_source(ps),
            line(ps, self.base.unwrap())
        )
    }

    fn no_base_message(
        &self,
        cx: &Investigation<'_, '_>,
        lhs: Node<'_>,
        rhs: Range,
        node: Node<'_>,
    ) -> String {
        let ps = cx.processed_source;
        let rhs_column = column(ps, rhs) as isize;
        let (used_indentation, expected_indentation) = match self.hash_pair_base_column {
            Some(base_column) => (
                rhs_column - base_column as isize,
                configured_indentation_width(cx),
            ),
            None => (
                rhs_column - indentation(ps, lhs) as isize,
                correct_indentation(cx, node),
            ),
        };
        let what = operation_description(node, rhs);
        format!(
            "Use {expected_indentation} (not {used_indentation}) spaces for indenting {what} spanning multiple lines."
        )
    }
}

impl Mei for MultilineMethodCallIndentation {
    fn relevant_node(&self, send_node: Node<'_>) -> bool {
        send_node.loc().dot.is_some()
    }

    fn right_hand_side(&self, cx: &Investigation<'_, '_>, send_node: Node<'_>) -> Option<Range> {
        let loc = send_node.loc();
        let dot = loc.dot?;
        let selector = loc.selector;
        if (send_node.is_dot() || send_node.is_safe_navigation())
            && let Some(selector) = selector
            && same_line(cx.processed_source, dot, selector)
        {
            Some(dot.join(selector))
        } else if let Some(selector) = selector {
            Some(selector)
        } else if send_node.is_implicit_call() {
            Some(dot.join(loc.begin?))
        } else {
            None
        }
    }

    fn offending_range<'a>(
        &mut self,
        cx: &Investigation<'a, '_>,
        node: Node<'a>,
        lhs: Node<'a>,
        rhs: Range,
        given_style: &str,
    ) -> Option<Range> {
        if !begins_its_line(cx.processed_source, rhs) {
            return None;
        }
        self.send_node = Some(node.id());
        let pair_ancestor = find_pair_ancestor(node);
        if let Some(_pair) = pair_ancestor
            && given_style == "aligned"
        {
            return self.check_hash_pair_indentation(cx, node, lhs, rhs);
        }
        if let Some(pair) = pair_ancestor
            && given_style == "indented"
            && find_base_receiver(node).is(NodeType::Hash)
        {
            return self.check_hash_pair_indented_style(cx, rhs, pair);
        }
        if Self::skip_for_context(node, pair_ancestor) {
            return None;
        }
        self.check_regular_indentation(cx, node, lhs, rhs, given_style)
    }

    fn message<'a>(
        &self,
        cx: &Investigation<'a, '_>,
        node: Node<'a>,
        lhs: Node<'a>,
        rhs: Range,
    ) -> String {
        if self.should_indent_relative_to_receiver(cx) {
            self.relative_to_receiver_message(cx, rhs)
        } else if self.should_align_with_base(cx) {
            self.align_with_base_message(cx.processed_source, rhs)
        } else {
            self.no_base_message(cx, lhs, rhs, node)
        }
    }

    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, range: Range) {
        if let Some(block_node) = self.send_node(cx).block_node() {
            self.correct_selector_only(corrector, cx, range);
            self.correct_block(corrector, cx, block_node);
        } else {
            alignment_corrector::correct(
                corrector,
                cx.processed_source,
                cx.config,
                Some(Target::Range(range)),
                self.column_delta,
                false,
            );
        }
    }
}

fn find_hash_pair_alignment_base(node: Node<'_>) -> Option<Range> {
    let base_receiver = find_base_receiver(node.receiver().unwrap());
    if !base_receiver.is(NodeType::Hash) {
        return None;
    }
    let first_call = first_call_has_a_dot(node);
    Some(dot_selector(first_call))
}

fn first_dot_alignment_base(ps: &ProcessedSource<'_>, node: Node<'_>, rhs: Range) -> Option<Range> {
    let rhs_source = ps.slice(rhs);
    if !(rhs_source.starts_with('.') || rhs_source.starts_with("&.")) {
        return None;
    }
    let first_call = first_call_has_a_dot(node);
    let dot = first_call.loc().dot?;
    if first_call == node {
        return None;
    }
    if let Some(after_block_base) = after_multiline_block_base(first_call, node) {
        return Some(after_block_base);
    }
    if !same_line(ps, dot, first_call.receiver().unwrap().source_range()) {
        return None;
    }
    Some(dot_selector(first_call))
}

fn after_multiline_block_base(first_call: Node<'_>, node: Node<'_>) -> Option<Range> {
    if !first_call.block_node()?.is_multiline() {
        return None;
    }
    let after_block = first_call.block_node()?.parent()?;
    if !(after_block.is_call() && after_block.loc().dot.is_some() && after_block != node) {
        return None;
    }
    Some(dot_selector(after_block))
}

fn inside_multiline_chain_arg(node: Node<'_>) -> bool {
    let Some(enclosing_call) = find_enclosing_chain_call(node) else {
        return false;
    };
    // `!same_line?(selector, receiver)`: no selector is never the same line.
    let Some(selector) = enclosing_call.loc().selector else {
        return true;
    };
    enclosing_call.line_of(selector)
        != enclosing_call.line_of(enclosing_call.receiver().unwrap().source_range())
}

fn find_enclosing_chain_call(node: Node<'_>) -> Option<Node<'_>> {
    let hash_ancestor = find_pair_ancestor(node)?.parent()?;
    let enclosing_call = hash_ancestor.parent();
    if !hash_arg_in_chain(enclosing_call, hash_ancestor) {
        return None;
    }
    enclosing_call
}

fn hash_arg_in_chain(call: Option<Node<'_>>, hash_node: Node<'_>) -> bool {
    call.is_some_and(|call| {
        call.is_call() && call.receiver() != Some(hash_node) && call.loc().dot.is_some()
    })
}

fn aligned_with_first_line_dot(ps: &ProcessedSource<'_>, node: Node<'_>, rhs: Range) -> bool {
    let rhs_source = ps.slice(rhs);
    if !(rhs_source.starts_with('.') || rhs_source.starts_with("&.")) {
        return false;
    }
    let first_call = first_call_has_a_dot(node);
    if Some(first_call) == node.receiver() {
        return false;
    }
    let dot = first_call.loc().dot.unwrap();
    line(ps, dot) == node.first_line() && column(ps, dot) == column(ps, rhs)
}

fn extra_indentation(
    cx: &Investigation<'_, '_>,
    given_style: &str,
    parent: Option<Node<'_>>,
) -> isize {
    if given_style != "indented_relative_to_receiver" {
        return 0;
    }
    match parent {
        Some(p) if matches!(p.ty(), NodeType::Splat | NodeType::Kwsplat) => {
            configured_indentation_width(cx) as isize - p.loc().operator.unwrap().len() as isize
        }
        _ => configured_indentation_width(cx) as isize,
    }
}

fn alignment_base(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
    rhs: Range,
    given_style: &str,
) -> Option<Range> {
    match given_style {
        "aligned" => semantic_alignment_base(ps, node, rhs)
            .or_else(|| syntactic_alignment_base(node, rhs)),
        "indented_relative_to_receiver" => Some(receiver_alignment_base(ps, node)),
        _ => None,
    }
}

fn syntactic_alignment_base(lhs: Node<'_>, rhs: Range) -> Option<Range> {
    if let Some(base) = kw_node_with_special_indentation(lhs) {
        return indented_keyword_expression(base).map(|n| n.source_range());
    }
    if let Some(base) = part_of_assignment_rhs(lhs, Some(rhs)) {
        return assignment_rhs(base).map(|n| n.source_range());
    }
    operation_rhs(lhs).map(|n| n.source_range())
}

fn semantic_alignment_base(ps: &ProcessedSource<'_>, node: Node<'_>, rhs: Range) -> Option<Range> {
    let rhs_source = ps.slice(rhs);
    if !(rhs_source.starts_with('.') || rhs_source.starts_with("&.")) {
        return None;
    }
    let node = semantic_alignment_node(ps, node)?;
    let selector = node.loc().selector?;
    let dot = node.loc().dot?;
    Some(dot.join(selector))
}

fn receiver_alignment_base(ps: &ProcessedSource<'_>, node: Node<'_>) -> Range {
    if let Some(hash_method_base) = find_hash_method_base_in_receiver_chain(ps, node) {
        return hash_method_base;
    }
    let first_call = first_call_has_a_dot(node);
    first_call.receiver().unwrap().source_range()
}

fn find_hash_method_base_in_receiver_chain(ps: &ProcessedSource<'_>, node: Node<'_>) -> Option<Range> {
    let mut receiver_chain = unwrap_block_node(node.receiver());
    while let Some(chain) = receiver_chain.filter(|c| c.is_call()) {
        let base_receiver = unwrap_block_node(chain.receiver());
        if alignment_base_for_chained_receiver(ps, chain, base_receiver) {
            return Some(dot_selector(chain));
        }
        receiver_chain = base_receiver;
    }
    None
}

fn alignment_base_for_chained_receiver(
    ps: &ProcessedSource<'_>,
    receiver_chain: Node<'_>,
    base_receiver: Option<Node<'_>>,
) -> bool {
    base_receiver.is_some_and(|b| b.is(NodeType::Hash))
        || method_on_receiver_last_line(ps, receiver_chain, base_receiver, NodeType::Begin)
}

fn semantic_alignment_node<'a>(ps: &ProcessedSource<'_>, node: Node<'a>) -> Option<Node<'a>> {
    if argument_in_method_call(node, ArgumentKind::WithParentheses).is_some() {
        return None;
    }
    get_dot_right_above(ps, node)
        .or_else(|| find_multiline_block_chain_node(node))
        .or_else(|| first_call_alignment_node(ps, node))
}

fn first_call_alignment_node<'a>(ps: &ProcessedSource<'_>, node: Node<'a>) -> Option<Node<'a>> {
    let node = first_call_has_a_dot(node);
    let base_receiver = find_base_receiver(node);
    if method_on_receiver_last_line(ps, node, Some(base_receiver), NodeType::Array) {
        return Some(node);
    }
    if line(ps, node.loc().dot.unwrap()) != node.first_line() {
        return None;
    }
    if method_on_receiver_last_line(ps, node, Some(base_receiver), NodeType::Begin) {
        return None;
    }
    Some(node)
}

fn method_on_receiver_last_line(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
    base_receiver: Option<Node<'_>>,
    ty: NodeType,
) -> bool {
    let Some(base_receiver) = base_receiver else {
        return false;
    };
    let Some(dot) = node.loc().dot else {
        return false;
    };
    same_line(ps, dot, base_receiver.source_range().end()) && base_receiver.is(ty)
}

fn get_dot_right_above<'a>(ps: &ProcessedSource<'_>, node: Node<'a>) -> Option<Node<'a>> {
    let node_dot = node.loc().dot.unwrap();
    node.ancestors().find(|a| {
        let Some(dot) = a.loc().dot else {
            return false;
        };
        line(ps, dot) + 1 == line(ps, node_dot) && column(ps, dot) == column(ps, node_dot)
    })
}

fn find_multiline_block_chain_node(node: Node<'_>) -> Option<Node<'_>> {
    if node.block_node().is_some() {
        return find_continuation_node(node);
    }
    handle_descendant_block(node)
}

fn find_continuation_node(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver()?;
    if single_line_block_receiver(receiver) {
        return Some(leftmost_call_on_same_line(receiver));
    }
    if !(receiver.is_call() && receiver.loc().dot.is_some()) {
        return None;
    }
    let receiver_receiver = receiver.receiver()?;
    if receiver_receiver.is(NodeType::Begin) && node.block_node()?.is_single_line() {
        return Some(receiver);
    }
    if receiver.line_of(receiver.loc().dot.unwrap()) <= receiver_receiver.last_line() {
        return None;
    }
    Some(receiver)
}

fn single_line_block_receiver(receiver: Node<'_>) -> bool {
    receiver.is_single_line() && receiver.is_any_block()
}

fn leftmost_call_on_same_line(node: Node<'_>) -> Node<'_> {
    let mut current = unwrap_block_node(Some(node)).unwrap();
    while let Some(receiver) = unwrap_block_node(current.receiver()) {
        let (Some(receiver_dot), Some(current_dot)) = (receiver.loc().dot, current.loc().dot) else {
            break;
        };
        if !(receiver.is_call() && receiver.line_of(receiver_dot) == current.line_of(current_dot)) {
            break;
        }
        current = receiver;
    }
    current
}

fn handle_descendant_block(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver()?;
    if single_line_block_receiver(receiver) {
        return Some(leftmost_call_on_same_line(receiver));
    }
    let block_node = node.descendants().find(|d| d.is_any_block())?;
    if !block_node.is_multiline() {
        return None;
    }
    if receiver.is_call() {
        Some(receiver)
    } else {
        block_node.parent()
    }
}

fn first_call_has_a_dot(node: Node<'_>) -> Node<'_> {
    let base = find_base_receiver(node);
    let mut node = base.parent().unwrap();
    while node.loc().dot.is_none() {
        node = node.parent().unwrap();
    }
    node
}

fn operation_rhs(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver()?;
    let operation_rhs = receiver
        .ancestors()
        .filter(|a| a.is(NodeType::Send))
        .find(|rhs| operator_rhs(*rhs, receiver))?;
    operation_rhs.first_argument()
}

fn operator_rhs(node: Node<'_>, receiver: Node<'_>) -> bool {
    node.is_operator_method()
        && node.has_arguments()
        && within_node(receiver.source_range(), node.first_argument().unwrap().source_range())
}
