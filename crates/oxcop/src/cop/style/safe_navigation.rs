//! Port of `lib/rubocop/cop/style/safe_navigation.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::nil_methods::nil_method;
use crate::cop::mixin::range_help::{
    range_between, range_with_surrounding_space, Side, SurroundingSpace,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str =
    "Use safe navigation (`&.`) instead of checking if an object exists before calling the method.";

static MODIFIER_IF_SAFE_NAVIGATION_CANDIDATE: NodePattern = NodePattern::new(
    "{
       (if {
             (send $_ {:nil? :!})
             $_
           } nil? $_)

       (if {
             (send (send $_ :nil?) :!)
             $_
           } $_ nil?)
     }",
);

static TERNARY_SAFE_NAVIGATION_CANDIDATE: NodePattern = NodePattern::new(
    "{
       (if (send $_ {:nil? :!}) nil $_)

       (if (send (send $_ :nil?) :!) $_ nil)

       (if $_ $_ nil)
     }",
);

static AND_WITH_RHS_OR: NodePattern = NodePattern::new("(and _ {or (begin or)})");
static NOT_NIL_CHECK: NodePattern = NodePattern::new("(send (send $_ :nil?) :!)");
static AND_INSIDE_BEGIN: NodePattern = NodePattern::new("`(begin and ...)");
static STRIP_BEGIN: NodePattern = NodePattern::new("{ (begin $!begin) $!(begin) }");

/// A part of an `and` chain: a clause or an operator (`collect_and_clauses`).
#[derive(Clone, Copy)]
enum Part<'a> {
    Node(Node<'a>),
    Op(Range),
}

impl Part<'_> {
    fn begin_pos(&self) -> usize {
        match self {
            Part::Node(n) => n.source_range().begin_pos,
            Part::Op(r) => r.begin_pos,
        }
    }
}

#[derive(Default)]
pub struct SafeNavigation {
    /// `ignore_node`/`ignored_node?` (identity, within one investigation).
    ignored: Vec<NodeId>,
}

impl Cop for SafeNavigation {
    fn name(&self) -> &'static str {
        "Style/SafeNavigation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.3)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If, NodeType::And]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::If) {
            self.on_if(cx, node);
        } else {
            self.on_and(cx, node);
        }
    }
}

impl SafeNavigation {
    fn on_if<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_else() || node.is_elsif() {
            return;
        }
        let (checked_variable, receiver, method_chain) = extract_parts_from_if(node);
        if !offending_node(cx, node, checked_variable, method_chain, receiver) {
            return;
        }
        let (Some(checked_variable), Some(receiver), Some(method_chain)) =
            (checked_variable, receiver, method_chain)
        else {
            return;
        };
        let Some(body) = extract_if_body(node) else {
            return;
        };
        let Some(method_call) = receiver.parent() else {
            return;
        };
        if dotless_operator_call(method_call) || method_call.is_double_colon() {
            return;
        }
        let removal_ranges = [begin_range(node, body), end_range(node, body)];
        self.report_offense(
            cx,
            node,
            method_chain,
            method_call,
            &removal_ranges,
            node.source_range(),
            |corrector| {
                if checked_variable.is_csend() {
                    corrector.replace(receiver.source_range(), checked_variable.source());
                }
                if !method_call.is_safe_navigation()
                    && let Some(dot) = method_call.loc().dot
                {
                    corrector.insert_before(dot, "&");
                }
            },
        );
    }

    fn on_and<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let src = cx.processed_source.buffer_source();
        let clauses = collect_and_clauses(node);
        for pair in clauses.windows(2) {
            let (Some(Part::Node(lhs)), Some(Part::Op(lhs_operator_range)), Some(Part::Node(rhs))) =
                (pair[0].first(), pair[0].get(1), pair[1].first())
            else {
                continue;
            };
            let (lhs, lhs_operator_range, rhs) = (*lhs, *lhs_operator_range, *rhs);
            let lhs_not_nil_check = NOT_NIL_CHECK.match_node(lhs).and_then(|c| c[0].node());
            let lhs_receiver = lhs_not_nil_check.unwrap_or(lhs);
            let stripped = STRIP_BEGIN.match_node(rhs).and_then(|c| c[0].node());
            let rhs_receiver = find_matching_receiver_invocation(stripped, Some(lhs_receiver));
            if !cx.cop_config_bool("ConvertCodeThatCanStartToReturnNil", false)
                && lhs_not_nil_check.is_some()
            {
                continue;
            }
            if !offending_node(cx, node, Some(lhs_receiver), Some(rhs), rhs_receiver) {
                continue;
            }
            let rhs_receiver = rhs_receiver.unwrap();
            let lhs_method_chain = find_method_chain(lhs_receiver);
            if !(lhs_method_chain.same_as(lhs_receiver) || lhs_not_nil_check.is_some()) {
                continue;
            }
            let right = SurroundingSpace::side(Side::Right);
            let removal_ranges = [
                range_with_surrounding_space(src, lhs.source_range(), right),
                range_with_surrounding_space(src, lhs_operator_range, right),
            ];
            let offense_range =
                range_between(lhs.source_range().begin_pos, rhs.source_range().end_pos);
            self.report_offense(
                cx,
                node,
                rhs,
                rhs_receiver,
                &removal_ranges,
                offense_range,
                |corrector| {
                    corrector.replace(rhs_receiver.source_range(), lhs_receiver.source());
                },
            );
            self.ignored.push(node.id());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn report_offense<'a>(
        &self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        rhs: Node<'a>,
        rhs_receiver: Node<'a>,
        removal_ranges: &[Range],
        offense_range: Range,
        extra: impl FnOnce(&mut Corrector),
    ) {
        let ignored = self.ignored.contains(&node.id());
        let ps = cx.processed_source;
        cx.add_offense_with_correction(offense_range, MSG, |corrector| {
            if ignored || AND_WITH_RHS_OR.is_match(node) {
                return;
            }
            for range in removal_ranges {
                corrector.remove(*range);
            }
            extra(corrector);
            handle_comments(ps, corrector, node, rhs);
            add_safe_nav_to_all_methods_in_chain(corrector, rhs_receiver, rhs);
        });
    }
}

fn find_method_chain(node: Node<'_>) -> Node<'_> {
    match node.parent() {
        Some(parent) if parent.is_call() => find_method_chain(parent),
        _ => node,
    }
}

fn collect_and_clauses(node: Node<'_>) -> Vec<Vec<Part<'_>>> {
    let mut nodes = and_parts(node);
    for and_node in node.descendants().filter(|d| d.is(NodeType::And)) {
        if and_node.ancestors().any(|a| a.is(NodeType::Block)) {
            continue;
        }
        nodes.extend(and_parts(and_node));
    }
    nodes.sort_by_key(Part::begin_pos);
    nodes.chunks(2).map(<[Part<'_>]>::to_vec).collect()
}

fn and_parts(node: Node<'_>) -> Vec<Part<'_>> {
    let mut parts = vec![Part::Op(node.loc().operator.unwrap())];
    let rhs = node.rhs().unwrap();
    if !AND_INSIDE_BEGIN.is_match(rhs) {
        parts.push(Part::Node(rhs));
    }
    let lhs = node.lhs().unwrap();
    if !(lhs.is(NodeType::And) || AND_INSIDE_BEGIN.is_match(lhs)) {
        parts.push(Part::Node(lhs));
    }
    parts
}

fn offending_node(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    lhs_receiver: Option<Node<'_>>,
    rhs: Option<Node<'_>>,
    rhs_receiver: Option<Node<'_>>,
) -> bool {
    if !matching_nodes(lhs_receiver, rhs_receiver) {
        return false;
    }
    let (Some(lhs_receiver), Some(rhs), Some(rhs_receiver)) = (lhs_receiver, rhs, rhs_receiver)
    else {
        return false;
    };
    if use_var_only_in_unless_modifier(node, lhs_receiver) {
        return false;
    }
    let max_chain_length = cx.cop_config_int("MaxChainLength").unwrap_or(2);
    if chain_length(rhs, rhs_receiver) > max_chain_length {
        return false;
    }
    let Some(method) = rhs_receiver.parent() else {
        return false;
    };
    if unsafe_method_used(cx, node, rhs, method) {
        return false;
    }
    !(rhs.is_send() && rhs.is_method("empty?"))
}

fn use_var_only_in_unless_modifier(node: Node<'_>, variable: Node<'_>) -> bool {
    node.is(NodeType::If) && node.is_unless() && !method_called(Some(variable))
}

fn extract_if_body(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_ternary() {
        node.branches()
            .into_iter()
            .flatten()
            .find(|branch| !branch.is(NodeType::Nil))
    } else {
        node.if_branch()
    }
}

fn dotless_operator_call(method_call: Node<'_>) -> bool {
    if dotless_operator_method(method_call) {
        return true;
    }
    let mut method_call = method_call;
    while let Some(parent) = method_call.parent().filter(|p| p.is_send()) {
        method_call = parent;
    }
    dotless_operator_method(method_call)
}

fn dotless_operator_method(method_call: Node<'_>) -> bool {
    if method_call.loc().dot.is_some() {
        return false;
    }
    method_call.is_method("[]") || method_call.is_method("[]=") || method_call.is_operator_method()
}

fn handle_comments(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>, rhs: Node<'_>) {
    let comments = comments(ps, node);
    if comments.is_empty() {
        return;
    }
    corrector.insert_before(rhs.source_range(), format!("{}\n", comments.join("\n")));
}

fn comments<'s>(ps: &'s ProcessedSource<'_>, node: Node<'_>) -> Vec<&'s str> {
    let mut comments = Vec::new();
    for (begin, end) in relevant_comment_ranges(node) {
        for line in begin..end {
            if let Some(comment) = ps.comment_at_line(line) {
                comments.push(comment.text);
            }
        }
    }
    comments
}

/// Line ranges (exclusive ends) between the node's children.
fn relevant_comment_ranges(node: Node<'_>) -> Vec<(usize, usize)> {
    let mut begin_pos = node.first_line();
    let end_pos = node.last_line();
    let mut ranges = Vec::new();
    for child in node.child_nodes() {
        ranges.push((begin_pos, child.first_line()));
        begin_pos = child.last_line();
    }
    ranges.push((begin_pos, end_pos));
    ranges
}

/// Returns `(checked_variable, matching_receiver, method_chain)`.
fn extract_parts_from_if(node: Node<'_>) -> (Option<Node<'_>>, Option<Node<'_>>, Option<Node<'_>>) {
    let captures = if node.is_ternary() {
        TERNARY_SAFE_NAVIGATION_CANDIDATE.match_node(node)
    } else {
        MODIFIER_IF_SAFE_NAVIGATION_CANDIDATE.match_node(node)
    };
    let (variable, receiver) = match &captures {
        Some(c) => (c[0].node(), c.get(1).and_then(|c| c.node())),
        None => (None, None),
    };
    let mut matching_receiver = find_matching_receiver_invocation(receiver, variable);
    if receiver.is_some_and(|r| {
        r.is_any(&[
            NodeType::Break,
            NodeType::Next,
            NodeType::Return,
            NodeType::Yield,
        ])
    }) {
        matching_receiver = None;
    }
    (variable, matching_receiver, receiver)
}

/// `Node#receiver` (`{(send $_ ...) (any_block (call $_ ...) ...)}`), or a
/// call's receiver.
fn node_receiver(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_call() || node.is_any_block() || node.is_any_def() {
        node.receiver()
    } else {
        None
    }
}

fn find_matching_receiver_invocation<'a>(
    method_chain: Option<Node<'a>>,
    checked_variable: Option<Node<'a>>,
) -> Option<Node<'a>> {
    let method_chain = method_chain?;
    let receiver = node_receiver(method_chain);
    if matching_nodes(receiver, checked_variable) {
        return receiver;
    }
    find_matching_receiver_invocation(receiver, checked_variable)
}

fn matching_nodes(left: Option<Node<'_>>, right: Option<Node<'_>>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(l), Some(r)) => l.same_as(r) || matching_call_nodes(l, r),
        _ => false,
    }
}

fn matching_call_nodes(left: Node<'_>, right: Node<'_>) -> bool {
    if !(left.is_call() && right.is_call()) {
        return false;
    }
    let (l, r) = (left.children(), right.children());
    let args_equal = l.len() == r.len()
        && l[2..].iter().zip(&r[2..]).all(|(a, b)| match (a.as_node(), b.as_node()) {
            (Some(a), Some(b)) => left.ast().node(a).same_as(right.ast().node(b)),
            (None, None) => a == b,
            _ => false,
        });
    left.method_name() == right.method_name()
        && matching_nodes(left.receiver(), right.receiver())
        && args_equal
}

fn chain_length(method_chain: Node<'_>, method: Node<'_>) -> i64 {
    let mut total = 0;
    for ancestor in method.ancestors().filter(|a| a.is_call()) {
        total += 1;
        if ancestor.same_as(method_chain) {
            break;
        }
    }
    total
}

fn unsafe_method_used(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    method_chain: Node<'_>,
    method: Node<'_>,
) -> bool {
    if unsafe_method(node, method) {
        return true;
    }
    for ancestor in method.ancestors().filter(|a| a.is_send()) {
        if !cx.config.cop_enabled("Lint/SafeNavigationChain") {
            return true;
        }
        if unsafe_method(node, ancestor) {
            return true;
        }
        if nil_method(cx, ancestor.method_name().unwrap_or_default()) {
            return true;
        }
        if ancestor.same_as(method_chain) {
            return false;
        }
    }
    false
}

fn unsafe_method(node: Node<'_>, send_node: Node<'_>) -> bool {
    if negated(send_node) {
        return true;
    }
    if node.is(NodeType::If) && node.is_ternary() {
        return false;
    }
    send_node.is_setter_method() || (!send_node.is_dot() && !send_node.is_safe_navigation())
}

fn negated(send_node: Node<'_>) -> bool {
    if method_called(Some(send_node)) {
        negated(send_node.parent().unwrap())
    } else {
        send_node.is_send() && send_node.is_method("!")
    }
}

fn method_called(send_node: Option<Node<'_>>) -> bool {
    send_node
        .and_then(|n| n.parent())
        .is_some_and(|p| p.is_send())
}

fn begin_range(node: Node<'_>, method_call: Node<'_>) -> Range {
    range_between(
        node.source_range().begin_pos,
        method_call.source_range().begin_pos,
    )
}

fn end_range(node: Node<'_>, method_call: Node<'_>) -> Range {
    range_between(
        method_call.source_range().end_pos,
        node.source_range().end_pos,
    )
}

fn add_safe_nav_to_all_methods_in_chain(
    corrector: &mut Corrector,
    start_method: Node<'_>,
    method_chain: Node<'_>,
) {
    for ancestor in start_method.ancestors() {
        if !(ancestor.is_call() || ancestor.is_any_block()) {
            break;
        }
        if !ancestor.is_send() || ancestor.is_operator_method() {
            continue;
        }
        if let Some(dot) = ancestor.loc().dot {
            corrector.insert_before(dot, "&");
        }
        if ancestor.same_as(method_chain) {
            break;
        }
    }
}
