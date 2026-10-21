//! Port of `lib/rubocop/cop/lint/ambiguous_block_association.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::{Cop, Corrector, Investigation};

const BLOCK_METHODS: &[&str] = &[
    "map",
    "collect",
    "flat_map",
    "collect_concat",
    "select",
    "filter",
    "find_all",
    "reject",
    "find",
    "detect",
    "each",
    "each_with_object",
    "each_with_index",
    "reduce",
    "inject",
    "sort_by",
    "min_by",
    "max_by",
    "group_by",
    "filter_map",
];

#[derive(Default)]
pub struct AmbiguousBlockAssociation;

impl Cop for AmbiguousBlockAssociation {
    fn name(&self) -> &'static str {
        "Lint/AmbiguousBlockAssociation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Send,
            NodeType::Csend,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() {
            on_block(cx, node);
        } else {
            on_send(cx, node);
        }
    }
}

fn on_send<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if !node.has_arguments() {
        return;
    }
    if !ambiguous_block_association(node) {
        return;
    }
    let last_argument = node.last_argument().unwrap();
    if node.is_parenthesized() || last_argument.is_lambda_or_proc() || allowed_method_pattern(cx, node)
    {
        return;
    }
    let message = message(node);
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        wrap_in_parentheses(corrector, node);
    });
}

fn on_block<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.has_braces() {
        return;
    }
    let send_node = node.send_node().unwrap();
    let Some(block_method_arg) = find_ambiguous_block_method(cx, node) else {
        return;
    };
    let message = format!(
        "`{}` is called without a block because the `do` block binds to `{}`. Use braces or extract to a variable.",
        block_method_arg.method_name().unwrap_or_default(),
        send_node.method_name().unwrap_or_default()
    );
    cx.add_offense(block_method_arg.source_range(), &message);
}

fn find_ambiguous_block_method<'a>(cx: &Investigation<'_, '_>, block_node: Node<'a>) -> Option<Node<'a>> {
    let send_node = block_node.send_node()?;
    if !(send_node.is_send() && send_node.has_arguments()) {
        return None;
    }
    if send_node.is_parenthesized() {
        return None;
    }
    let block_method_arg = find_block_method_arg(send_node)?;
    if allowed_method(cx, block_method_arg.method_name().unwrap_or_default())
        || matches_allowed_pattern(cx, block_method_arg.source())
    {
        return None;
    }
    Some(block_method_arg)
}

fn ambiguous_block_association(send_node: Node<'_>) -> bool {
    let last_argument = send_node.last_argument().unwrap();
    last_argument.is_any_block()
        && !last_argument
            .send_node()
            .is_some_and(|s| s.has_arguments())
}

fn allowed_method_pattern(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let last_argument = node.last_argument().unwrap();
    // `assignment?` on a send is `setter_method?`.
    node.is_assignment()
        || node.is_setter_method()
        || node.is_operator_method()
        || node.is_method("[]")
        || allowed_method(cx, last_argument.method_name().unwrap_or_default())
        || matches_allowed_pattern(
            cx,
            last_argument.send_node().map(|s| s.source()).unwrap_or_default(),
        )
}

fn message(send_node: Node<'_>) -> String {
    let block_param = send_node.last_argument().unwrap();
    format!(
        "Parenthesize the param `{}` to make sure that the block will be associated with the `{}` method call.",
        block_param.source(),
        block_param.send_node().map(|s| s.source()).unwrap_or_default()
    )
}

fn find_block_method_arg(send_node: Node<'_>) -> Option<Node<'_>> {
    let end_pos = send_node.source_range().end_pos;
    send_node.last_argument()?.each_node().find(|node| {
        node.is_call()
            && node.source_range().end_pos == end_pos
            && node
                .method_name()
                .is_some_and(|m| BLOCK_METHODS.contains(&m))
            && !node.has_arguments()
    })
}

fn wrap_in_parentheses(corrector: &mut Corrector, node: Node<'_>) {
    let range = node
        .loc()
        .selector
        .unwrap()
        .end()
        .join(node.first_argument().unwrap().source_range().begin());
    corrector.remove(range);
    corrector.insert_before(range, "(");
    corrector.insert_after(node.last_argument().unwrap().source_range(), ")");
}
