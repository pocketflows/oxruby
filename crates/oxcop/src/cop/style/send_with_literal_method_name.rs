//! Port of `lib/rubocop/cop/style/send_with_literal_method_name.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

const RESERVED_WORDS: &[&str] = &[
    "BEGIN", "END", "alias", "and", "begin", "break", "case", "class", "def", "defined?", "do",
    "else", "elsif", "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not",
    "or", "redo", "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless",
    "until", "when", "while", "yield",
];

#[derive(Default)]
pub struct SendWithLiteralMethodName;

impl Cop for SendWithLiteralMethodName {
    fn name(&self) -> &'static str {
        "Style/SendWithLiteralMethodName"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["public_send", "send", "__send__"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if cx.cop_config_bool("AllowSend", false) && !node.is_method("public_send") {
            return;
        }
        let Some(first_argument) = node.first_argument() else {
            return;
        };
        let (method_name, is_symbol) = match (first_argument.ty(), first_argument.value()) {
            (NodeType::Sym, Some(Child::Sym(name))) => (name.as_str(), true),
            (NodeType::Str, Some(Child::Str(name))) => (name.as_str(), false),
            _ => return,
        };
        let offense_range = node
            .loc()
            .selector
            .unwrap()
            .join(node.source_range().end());
        // `RESERVED_WORDS` holds symbols, so string names are never reserved.
        if regex!(r"\A[a-zA-Z_][a-zA-Z0-9_]*[!?]?\z")
            .find(method_name)
            .is_none()
            || (is_symbol && RESERVED_WORDS.contains(&method_name))
        {
            return;
        }
        let message = format!("Use `{method_name}` method call directly instead.");
        let arguments = node.arguments();
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            if arguments.len() == 1 {
                corrector.replace(offense_range, method_name);
            } else {
                corrector.replace(node.loc().selector.unwrap(), method_name);
                corrector.remove(removal_argument_range(first_argument, arguments[1]));
            }
        });
    }
}

fn removal_argument_range(first_argument: Node<'_>, second_argument: Node<'_>) -> Range {
    first_argument
        .source_range()
        .begin()
        .join(second_argument.source_range().begin())
}
