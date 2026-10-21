//! Port of `lib/rubocop/cop/style/partition_instead_of_double_select.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};

const SELECT_METHODS: &[&str] = &["select", "filter", "find_all"];
const CANDIDATE_METHODS: &[&str] = &["select", "filter", "find_all", "reject"];

fn is_select(name: Option<&str>) -> bool {
    name.is_some_and(|n| SELECT_METHODS.contains(&n))
}

/// Ruby's structural `==` on optional nodes.
fn same(a: Option<Node<'_>>, b: Option<Node<'_>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b),
        _ => false,
    }
}

#[derive(Default)]
pub struct PartitionInsteadOfDoubleSelect;

impl Cop for PartitionInsteadOfDoubleSelect {
    fn name(&self) -> &'static str {
        "Style/PartitionInsteadOfDoubleSelect"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Send,
            NodeType::Csend,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        CANDIDATE_METHODS
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() {
            if !node
                .method_name()
                .is_some_and(|m| CANDIDATE_METHODS.contains(&m))
            {
                return;
            }
        } else if !node
            .last_argument()
            .is_some_and(|a| a.is(NodeType::BlockPass))
        {
            return;
        }
        find_and_register_offense(cx, node);
    }
}

fn find_and_register_offense<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(container) = node_container(node) else {
        return;
    };
    let Some(sibling_container) = container.left_sibling() else {
        return;
    };
    let Some(sibling) = find_matching_candidate(node, sibling_container) else {
        return;
    };
    let message = format!(
        "Use `partition` instead of consecutive `{}` and `{}` calls.",
        sibling.method_name().unwrap_or(""),
        node.method_name().unwrap_or("")
    );
    let ps = cx.processed_source;
    cx.add_offense_with_correction(container.source_range(), &message, |corrector| {
        if !(container.is(NodeType::Lvasgn) && sibling_container.is(NodeType::Lvasgn)) {
            return;
        }
        let (select_var, reject_var, partition_node) = if complementary_pair(node, sibling) {
            let (select_var, reject_var) =
                complementary_variable_order(sibling, container, sibling_container);
            let partition_node = if is_select(sibling.method_name()) {
                sibling
            } else {
                container.child_from_end(1).unwrap()
            };
            (select_var, reject_var, partition_node)
        } else {
            negation_partition_args(node, sibling, container, sibling_container)
        };
        let replacement = format!(
            "{select_var}, {reject_var} = {}",
            build_partition_call(partition_node)
        );
        corrector.replace(sibling_container.source_range(), replacement);
        corrector.remove(range_by_whole_lines(ps, container.source_range(), true));
    });
}

fn node_container(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if parent.is(NodeType::Begin) {
        Some(node)
    } else if parent.is_assignment() && parent.parent().is_some_and(|p| p.is(NodeType::Begin)) {
        Some(parent)
    } else {
        None
    }
}

fn find_matching_candidate<'a>(node: Node<'a>, sibling_container: Node<'a>) -> Option<Node<'a>> {
    let sibling = extract_candidate(sibling_container)?;
    if !same(node.receiver(), sibling.receiver()) || !matching_pair(node, sibling) {
        return None;
    }
    Some(sibling)
}

fn matching_pair(node: Node<'_>, sibling: Node<'_>) -> bool {
    (complementary_pair(node, sibling) && equivalent_predicate(node, sibling))
        || (node.method_name() == sibling.method_name() && negated_predicate(node, sibling))
}

fn extract_candidate(container: Node<'_>) -> Option<Node<'_>> {
    extract_block(container).or_else(|| extract_block_pass_send(container))
}

fn extract_block(container: Node<'_>) -> Option<Node<'_>> {
    if container.is_any_block() {
        Some(container)
    } else if container.is_assignment() {
        container.child_from_end(1).filter(|r| r.is_any_block())
    } else {
        None
    }
}

fn extract_block_pass_send(container: Node<'_>) -> Option<Node<'_>> {
    let node = if container.is_assignment() {
        container.child_from_end(1)?
    } else {
        container
    };
    if !node.is_call()
        || !node
            .last_argument()
            .is_some_and(|a| a.is(NodeType::BlockPass))
    {
        return None;
    }
    Some(node)
}

fn complementary_pair(node1: Node<'_>, node2: Node<'_>) -> bool {
    let (m1, m2) = (node1.method_name(), node2.method_name());
    (is_select(m1) && m2 == Some("reject")) || (m1 == Some("reject") && is_select(m2))
}

fn equivalent_predicate(node1: Node<'_>, node2: Node<'_>) -> bool {
    match (node1.is_any_block(), node2.is_any_block()) {
        (true, true) => same_block_contents(node1, node2),
        (true, false) => block_matches_block_pass(node1, node2),
        (false, true) => block_matches_block_pass(node2, node1),
        (false, false) => same(node1.last_argument(), node2.last_argument()),
    }
}

fn same_block_contents(block1: Node<'_>, block2: Node<'_>) -> bool {
    if block1.ty() != block2.ty() {
        return false;
    }
    if block1.is(NodeType::Block) && !same(block1.child(1), block2.child(1)) {
        return false;
    }
    same(block1.body(), block2.body())
}

/// `symbol_proc_method?`:
/// `(block _ (args (arg _name)) (send (lvar _name) $_method_name))`.
fn symbol_proc_method<'a>(block_node: Node<'a>) -> Option<&'a str> {
    if !block_node.is(NodeType::Block) {
        return None;
    }
    let args = block_node.child(1)?;
    if args.children().len() != 1 {
        return None;
    }
    let arg = args.child(0).filter(|a| a.is(NodeType::Arg))?;
    let body = block_node.child(2)?;
    if !(body.is_send() && body.children().len() == 2) {
        return None;
    }
    let receiver = body.receiver().filter(|r| r.is(NodeType::Lvar))?;
    if receiver.child_sym(0) != arg.name() {
        return None;
    }
    body.method_name()
}

fn block_matches_block_pass(block_node: Node<'_>, send_node: Node<'_>) -> bool {
    let Some(method_name) = symbol_proc_method(block_node) else {
        return false;
    };
    send_node
        .last_argument()
        .and_then(|a| a.child(0))
        .is_some_and(|sym| sym.is(NodeType::Sym) && sym.child_sym(0) == Some(method_name))
}

fn negated_predicate(node1: Node<'_>, node2: Node<'_>) -> bool {
    if !(node1.is_any_block() && node2.is_any_block()) || node1.ty() != node2.ty() {
        return false;
    }
    if node1.is(NodeType::Block) && !same(node1.child(1), node2.child(1)) {
        return false;
    }
    negated_body(node1.body(), node2.body()) || negated_body(node2.body(), node1.body())
}

fn negated_body(body1: Option<Node<'_>>, body2: Option<Node<'_>>) -> bool {
    body1.is_some_and(|b| b.is_send() && b.is_method("!") && same(b.receiver(), body2))
}

fn complementary_variable_order<'a>(
    sibling: Node<'a>,
    container: Node<'a>,
    sibling_container: Node<'a>,
) -> (&'a str, &'a str) {
    let a = sibling_container.name().unwrap_or("");
    let b = container.name().unwrap_or("");
    if is_select(sibling.method_name()) {
        (a, b)
    } else {
        (b, a)
    }
}

fn negation_partition_args<'a>(
    node: Node<'a>,
    sibling: Node<'a>,
    container: Node<'a>,
    sibling_container: Node<'a>,
) -> (&'a str, &'a str, Node<'a>) {
    let node_is_negated = negated_body(node.body(), sibling.body());
    let select = is_select(node.method_name());
    let node_is_truthy = select != node_is_negated;
    let partition_node = if node_is_negated { sibling } else { node };
    let a = container.name().unwrap_or("");
    let b = sibling_container.name().unwrap_or("");
    if node_is_truthy {
        (a, b, partition_node)
    } else {
        (b, a, partition_node)
    }
}

fn build_partition_call(node: Node<'_>) -> String {
    let source = node.source();
    let send_node = if node.is_any_block() {
        node.send_node().unwrap()
    } else {
        node
    };
    let selector = send_node.loc().selector.unwrap();
    let offset = node.source_range().begin_pos;
    let method_start = selector.begin_pos - offset;
    let method_end = selector.end_pos - offset;
    format!(
        "{}partition{}",
        &source[..method_start],
        &source[method_end..]
    )
}
