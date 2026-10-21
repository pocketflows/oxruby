//! Port of `lib/rubocop/cop/style/string_chars.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const BAD_ARGUMENTS: &[&str] = &["//", "''", "\"\""];

#[derive(Default)]
pub struct StringChars;

impl Cop for StringChars {
    fn name(&self) -> &'static str {
        "Style/StringChars"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["split"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let args = node.arguments();
        if args.len() != 1 || !BAD_ARGUMENTS.contains(&args[0].source()) {
            return;
        }
        let range = range_between(
            node.loc().selector.unwrap().begin_pos,
            node.source_range().end_pos,
        );
        let message = format!("Use `chars` instead of `{}`.", cx.source(range));
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, "chars");
        });
    }
}
