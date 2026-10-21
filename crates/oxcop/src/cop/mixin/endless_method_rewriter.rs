//! Port of `lib/rubocop/cop/mixin/endless_method_rewriter.rb`.

use serde_yaml_ng::Value;

use crate::ast::Node;
use crate::cop::mixin::util::indent;
use crate::cop::{Corrector, Investigation};

/// `correct_to_multiline(corrector, node)`; `indentation_width` is
/// [`configured_indentation_width`].
pub fn correct_to_multiline(corrector: &mut Corrector, node: Node<'_>, indentation_width: i64) {
    let replacement = [
        format!(
            "def {}{}{}",
            receiver(node),
            node.method_name().unwrap_or(""),
            arguments(node, "")
        ),
        format!(
            "{}{}",
            indent(node, indentation_width as isize),
            node.body().map_or("", |b| b.source())
        ),
        format!("{}end", indent(node, 0)),
    ]
    .join("\n");
    corrector.replace(node.source_range(), replacement);
}

/// `receiver(node)`: `self.` (or `self::`) for singleton methods.
pub fn receiver(node: Node<'_>) -> String {
    match (node.receiver(), node.loc().operator) {
        (Some(receiver), Some(operator)) => {
            format!("{}{}", receiver.source(), node.range_source(operator))
        }
        (Some(receiver), None) => receiver.source().to_owned(),
        _ => String::new(),
    }
}

/// `arguments(node, missing = '')`.
pub fn arguments<'a>(node: Node<'a>, missing: &'a str) -> &'a str {
    match node.arguments_node() {
        Some(args) if args.child_nodes().next().is_some() => args.source(),
        _ => missing,
    }
}

/// `configured_indentation_width`.
pub fn configured_indentation_width(cx: &Investigation<'_, '_>) -> i64 {
    cx.cop_config_int("IndentationWidth")
        .or_else(|| {
            cx.config
                .for_cop("Layout/IndentationWidth")
                .get("Width")
                .and_then(Value::as_i64)
        })
        .unwrap_or(2)
}
