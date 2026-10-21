//! Port of `lib/rubocop/cop/style/random_with_offset.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer ranges when generating random numbers instead of integers with offsets.";

static INTEGER_OP_RAND: NodePattern = NodePattern::new(
    "(send
      int {:+ :-}
      (send
        {nil? (const {nil? cbase} :Random) (const {nil? cbase} :Kernel)}
        :rand
        {int (range int int)}))",
);
static RAND_OP_INTEGER: NodePattern = NodePattern::new(
    "(send
      (send
        {nil? (const {nil? cbase} :Random) (const {nil? cbase} :Kernel)}
        :rand
        {int (range int int)})
      {:+ :-}
      int)",
);
static RAND_MODIFIED: NodePattern = NodePattern::new(
    "(send
      (send
        {nil? (const {nil? cbase} :Random) (const {nil? cbase} :Kernel)}
        :rand
        {int (range int int)})
      {:succ :pred :next})",
);

#[derive(Default)]
pub struct RandomWithOffset;

impl Cop for RandomWithOffset {
    fn name(&self) -> &'static str {
        "Style/RandomWithOffset"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["+", "-", "succ", "pred", "next"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.receiver().is_none() {
            return;
        }
        let corrected = if INTEGER_OP_RAND.is_match(node) {
            corrected_integer_op_rand(node)
        } else if RAND_OP_INTEGER.is_match(node) {
            corrected_rand_op_integer(node)
        } else if RAND_MODIFIED.is_match(node) {
            corrected_rand_modified(node)
        } else {
            return;
        };
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            if let Some(corrected) = corrected {
                corrector.replace(node.source_range(), corrected);
            }
        });
    }
}

/// `random_call`: `{(send (send $_ _ $_) ...) (send _ _ (send $_ _ $_))}`.
fn random_call(node: Node<'_>) -> (Option<Node<'_>>, Node<'_>) {
    let rand = match node.receiver() {
        Some(r) if r.is_send() => r,
        _ => node.first_argument().unwrap(),
    };
    (rand.receiver(), rand.first_argument().unwrap())
}

fn prefix_from_prefix_node(node: Option<Node<'_>>) -> String {
    match node {
        Some(n) => format!("{}.rand", n.source()),
        None => "rand".to_owned(),
    }
}

fn to_int(node: Node<'_>) -> i128 {
    node.int_value().map_or(0, i128::from)
}

fn boundaries_from_random_node(random_node: Node<'_>) -> (i128, i128) {
    match random_node.ty() {
        NodeType::Int => (0, to_int(random_node) - 1),
        NodeType::Irange => (
            to_int(random_node.child(0).unwrap()),
            to_int(random_node.child(1).unwrap()),
        ),
        _ => (
            to_int(random_node.child(0).unwrap()),
            to_int(random_node.child(1).unwrap()) - 1,
        ),
    }
}

fn corrected_integer_op_rand(node: Node<'_>) -> Option<String> {
    let (prefix_node, random_node) = random_call(node);
    let prefix = prefix_from_prefix_node(prefix_node);
    let (left, right) = boundaries_from_random_node(random_node);
    let offset = to_int(node.receiver().unwrap());
    Some(if node.is_method("+") {
        format!("{prefix}({}..{})", offset + left, offset + right)
    } else {
        format!("{prefix}({}..{})", offset - right, offset - left)
    })
}

fn corrected_rand_op_integer(node: Node<'_>) -> Option<String> {
    let (prefix_node, random_node) = random_call(node);
    let prefix = prefix_from_prefix_node(prefix_node);
    let (left, right) = boundaries_from_random_node(random_node);
    let offset = to_int(node.first_argument().unwrap());
    Some(if node.is_method("+") {
        format!("{prefix}({}..{})", left + offset, right + offset)
    } else {
        format!("{prefix}({}..{})", left - offset, right - offset)
    })
}

fn corrected_rand_modified(node: Node<'_>) -> Option<String> {
    let (prefix_node, random_node) = random_call(node);
    let prefix = prefix_from_prefix_node(prefix_node);
    let (left, right) = boundaries_from_random_node(random_node);
    match node.method_name()? {
        "succ" | "next" => Some(format!("{prefix}({}..{})", left + 1, right + 1)),
        "pred" => Some(format!("{prefix}({}..{})", left - 1, right - 1)),
        _ => None,
    }
}
