//! Port of `lib/rubocop/cop/style/raise_args.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const ACCEPTABLE_ARG_TYPES: &[NodeType] = &[
    NodeType::Hash,
    NodeType::ForwardedRestarg,
    NodeType::Splat,
    NodeType::ForwardedKwrestarg,
    NodeType::ForwardedArgs,
];

#[derive(Default)]
pub struct RaiseArgs;

impl Cop for RaiseArgs {
    fn name(&self) -> &'static str {
        "Style/RaiseArgs"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["raise", "fail"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_command("raise") || node.is_command("fail")) {
            return;
        }
        match style(cx) {
            "compact" => check_compact(cx, node),
            "exploded" => check_exploded(cx, node),
            _ => {}
        }
    }
}

fn check_compact<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.arguments().len() <= 1 {
        return;
    }
    let exception = node.first_argument().unwrap();
    if exception.is_send()
        && exception
            .first_argument()
            .is_some_and(|a| a.is(NodeType::Hash))
    {
        return;
    }
    let message = format!(
        "Provide an exception object as an argument to `{}`.",
        node.method_name().unwrap_or("")
    );
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        if let Some(replacement) = correction_exploded_to_compact(node) {
            corrector.replace(node.source_range(), replacement);
        }
    });
}

fn check_exploded<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let args = node.arguments();
    if args.len() != 1 {
        return;
    }
    let first_arg = args[0];
    if !use_new_method(first_arg) || acceptable_exploded_args(&first_arg.arguments()) {
        return;
    }
    if allowed_non_exploded_type(cx, first_arg) {
        return;
    }
    let message = format!(
        "Provide an exception class and message as arguments to `{}`.",
        node.method_name().unwrap_or("")
    );
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        corrector.replace(node.source_range(), correction_compact_to_exploded(node));
    });
}

fn correction_compact_to_exploded(node: Node<'_>) -> String {
    let first = node.first_argument().unwrap();
    let arguments: Vec<&str> = [first.child(0), first.child(2)]
        .into_iter()
        .flatten()
        .map(|n| n.source())
        .collect();
    let arguments = arguments.join(", ");
    let method = node.method_name().unwrap_or("");
    if node.parent().is_some_and(requires_parens) {
        format!("{method}({arguments})")
    } else {
        format!("{method} {arguments}")
    }
}

fn correction_exploded_to_compact(node: Node<'_>) -> Option<String> {
    let args = node.arguments();
    let (exception_node, message_nodes) = args.split_first()?;
    if message_nodes.len() > 1 {
        return None;
    }
    let argument = message_nodes.first()?.source();
    let exception_class = exception_node
        .receiver()
        .map_or(exception_node.source(), |r| r.source());
    let method = node.method_name().unwrap_or("");
    Some(if node.parent().is_some_and(requires_parens) {
        format!("{method}({exception_class}.new({argument}))")
    } else {
        format!("{method} {exception_class}.new({argument})")
    })
}

fn use_new_method(first_arg: Node<'_>) -> bool {
    first_arg.is_send() && first_arg.receiver().is_some() && first_arg.is_method("new")
}

fn acceptable_exploded_args(args: &[Node<'_>]) -> bool {
    match args {
        [] => false,
        [arg] => ACCEPTABLE_ARG_TYPES.contains(&arg.ty()),
        _ => true,
    }
}

fn allowed_non_exploded_type(cx: &Investigation<'_, '_>, arg: Node<'_>) -> bool {
    let Some(name) = arg.receiver().and_then(|r| r.const_name()) else {
        return false;
    };
    cx.cop_config_list("AllowedCompactTypes")
        .iter()
        .any(|t| *t == name)
}

fn requires_parens(parent: Node<'_>) -> bool {
    parent.is_operator_keyword() || (parent.is(NodeType::If) && parent.is_ternary())
}
