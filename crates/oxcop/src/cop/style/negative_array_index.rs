//! Port of `lib/rubocop/cop/style/negative_array_index.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

const PRESERVING_METHODS: &[&str] = &["sort", "reverse", "shuffle", "rotate"];

#[derive(Default)]
pub struct NegativeArrayIndex;

impl Cop for NegativeArrayIndex {
    fn name(&self) -> &'static str {
        "Style/NegativeArrayIndex"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(index_arg) = node.first_argument() else {
            return;
        };
        let Some(array_receiver) = node.receiver() else {
            return;
        };
        let range_node = if index_arg.is(NodeType::Begin) {
            index_arg.child(0)
        } else {
            Some(index_arg)
        };
        if let Some(range_node) = range_node
            && range_with_length_subtraction(range_node, array_receiver)
        {
            handle_range_pattern(cx, array_receiver.source(), range_node, index_arg);
            return;
        }
        // handle_simple_index_pattern
        let Some((length_receiver, negative_index)) = length_subtraction(Some(index_arg)) else {
            return;
        };
        if negative_index <= 0 || !receivers_match(length_receiver, array_receiver) {
            return;
        }
        let receiver = array_receiver.source();
        let range = index_arg.source_range();
        let message = format!(
            "Use `{receiver}[-{negative_index}]` instead of `{receiver}[{}]`.",
            index_arg.source()
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, format!("-{negative_index}"));
        });
    }
}

/// `(send (send $_ {:length :size :count}) :- (int $_))`
fn length_subtraction(node: Option<Node<'_>>) -> Option<(Option<Node<'_>>, i64)> {
    let node = node?;
    if !(node.is_send() && node.is_method("-") && node.children().len() == 3) {
        return None;
    }
    let length = node.receiver()?;
    if !(length.is_send()
        && length.children().len() == 2
        && matches!(length.method_name(), Some("length" | "size" | "count")))
    {
        return None;
    }
    let index = node.first_argument()?.int_value()?;
    Some((length.receiver(), index))
}

/// `extract_inner_end`: the only child, else the node.
fn extract_inner_end(node: Node<'_>) -> Option<Node<'_>> {
    match node.children() {
        [Child::Node(id)] => Some(node.ast().node(*id)),
        [_] => None,
        _ => Some(node),
    }
}

fn range_with_length_subtraction(range_node: Node<'_>, array_receiver: Node<'_>) -> bool {
    if !range_node.is_range() {
        return false;
    }
    let (Some(range_start), Some(range_end)) = (range_node.child(0), range_node.child(1)) else {
        return false;
    };
    if !preserving_method(range_start) {
        return false;
    }
    let Some((length_receiver, negative_index)) = length_subtraction(extract_inner_end(range_end))
    else {
        return false;
    };
    negative_index > 0
        && preserving_method(array_receiver)
        && length_receiver.is_some_and(|l| l.source() == array_receiver.source())
}

fn handle_range_pattern<'a>(
    cx: &mut Investigation<'a, '_>,
    receiver: &str,
    range_node: Node<'a>,
    index_arg: Node<'a>,
) {
    let range_end = range_node.child(1).unwrap();
    let inner_end = extract_inner_end(range_end).unwrap();
    let (_, negative_index) = length_subtraction(Some(inner_end)).unwrap();
    let range_op = if range_node.is(NodeType::Erange) {
        "..."
    } else {
        ".."
    };
    let range_start = range_node.child(0).unwrap().source();
    let end_expression = if range_end.is(NodeType::Begin) {
        range_end.source()
    } else {
        inner_end.source()
    };
    let range_without_parens = format!("{range_start}{range_op}{end_expression}");
    let has_parentheses = index_arg.is(NodeType::Begin);
    let (current, start, index, replacement) = if has_parentheses {
        (
            format!("{receiver}[({range_without_parens})]"),
            format!("({range_start}"),
            format!("{negative_index})"),
            format!("({range_start}{range_op}-{negative_index})"),
        )
    } else {
        (
            format!("{receiver}[{range_without_parens}]"),
            range_start.to_owned(),
            negative_index.to_string(),
            format!("{range_start}{range_op}-{negative_index}"),
        )
    };
    let message =
        format!("Use `{receiver}[{start}{range_op}-{index}]` instead of `{current}`.");
    cx.add_offense_with_correction(range_end.source_range(), &message, |corrector| {
        corrector.replace(index_arg.source_range(), &replacement);
    });
}

fn receivers_match(length_receiver: Option<Node<'_>>, array_receiver: Node<'_>) -> bool {
    let Some(length_receiver) = length_receiver else {
        return array_receiver.is(NodeType::SelfNode);
    };
    if !(preserving_method(array_receiver) && preserving_method(length_receiver)) {
        return false;
    }
    if length_receiver.source() == array_receiver.source() {
        return true;
    }
    extract_base_receiver(array_receiver).is_some()
}

fn extract_base_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let receiver = node.receiver()?;
    if receiver.receiver().is_none() {
        return Some(receiver);
    }
    extract_base_receiver(receiver)
}

fn preserving_method(node: Node<'_>) -> bool {
    let Some(receiver) = node.receiver() else {
        return true;
    };
    if !node
        .method_name()
        .is_some_and(|m| PRESERVING_METHODS.contains(&m))
    {
        return false;
    }
    preserving_method(receiver)
}
