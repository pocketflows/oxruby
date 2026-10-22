//! Port of rubocop-rspec's `lib/rubocop/cop/rspec/empty_line_after_hook.rb`.
//!
//! Upstream also includes `ConfigurableEnforcedStyle`, which this cop never
//! uses.

use crate::ast::{Node, NodeType};
use crate::cop::rspec::language::Language;
use crate::cop::rspec::mixin::empty_line_separation::missing_separating_line_offense;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Add an empty line after `%<hook>s`.";

#[derive(Default)]
pub struct EmptyLineAfterHook;

impl Cop for EmptyLineAfterHook {
    fn name(&self) -> &'static str {
        "RSpec/EmptyLineAfterHook"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    /// `on_block`, aliased as `on_numblock` and `on_itblock`.
    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let language = Language::new(cx.config);
        if !language.hook(node) {
            return;
        }
        if cx.cop_config_bool("AllowConsecutiveOneLiners", false)
            && chained_single_line_hooks(&language, node)
        {
            return;
        }

        missing_separating_line_offense(cx, node, |method| MSG.replace("%<hook>s", method));
    }
}

fn chained_single_line_hooks(language: &Language<'_>, node: Node<'_>) -> bool {
    let Some(next_node) = node.right_sibling() else {
        return false;
    };

    language.hook(next_node) && node.is_single_line() && next_node.is_single_line()
}
