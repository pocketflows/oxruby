//! Port of rubocop-rspec's `lib/rubocop/cop/rspec/empty_line_after_example.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::rspec::language::Language;
use crate::cop::rspec::mixin::empty_line_separation::missing_separating_line_offense;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Add an empty line after `%<example>s`.";

#[derive(Default)]
pub struct EmptyLineAfterExample;

impl Cop for EmptyLineAfterExample {
    fn name(&self) -> &'static str {
        "RSpec/EmptyLineAfterExample"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    /// `on_block` (upstream has no numblock/itblock aliases).
    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let language = Language::new(cx.config);
        if !language.example(node) {
            return;
        }
        if allowed_one_liner(cx, &language, node) {
            return;
        }

        missing_separating_line_offense(cx, node, |method| MSG.replace("%<example>s", method));
    }
}

fn allowed_one_liner(cx: &Investigation<'_, '_>, language: &Language<'_>, node: Node<'_>) -> bool {
    consecutive_one_liner(language, node) && allow_consecutive_one_liners(cx)
}

fn allow_consecutive_one_liners(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowConsecutiveOneLiners", false)
}

fn consecutive_one_liner(language: &Language<'_>, node: Node<'_>) -> bool {
    node.is_single_line() && next_one_line_example(language, node)
}

fn next_one_line_example(language: &Language<'_>, node: Node<'_>) -> bool {
    let Some(next_sibling) = node.right_sibling() else {
        return false;
    };
    if !language.example(next_sibling) {
        return false;
    }

    next_sibling.is_single_line()
}
