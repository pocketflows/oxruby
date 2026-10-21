//! Port of `lib/rubocop/cop/security/io_methods.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct IoMethods;

impl Cop for IoMethods {
    fn name(&self) -> &'static str {
        "Security/IoMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["read", "binread", "write", "binwrite", "foreach", "readlines"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver() else {
            return;
        };
        if receiver.source() != "IO" {
            return;
        }
        if let Some(argument) = node.first_argument()
            && argument.is(NodeType::Str)
            && argument
                .str_content()
                .is_some_and(|value| ruby_strip(value).starts_with('|'))
        {
            return;
        }
        let method_name = node.method_name().unwrap_or("");
        let message = format!("`File.{method_name}` is safer than `IO.{method_name}`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(receiver.source_range(), "File");
        });
    }
}

/// Ruby's `String#strip` (whitespace and NUL).
fn ruby_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\0' || c == '\x0b')
}
