//! Port of `lib/rubocop/cop/style/nested_ternary_operator.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::{SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Ternary operators must not be nested. Prefer `if` or `else` constructs instead.";

#[derive(Default)]
pub struct NestedTernaryOperator {
    ignored: IgnoredNodes,
}

impl Cop for NestedTernaryOperator {
    fn name(&self) -> &'static str {
        "Style/NestedTernaryOperator"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_ternary() {
            return;
        }
        let src = cx.processed_source.buffer_source();
        let nested: Vec<Node<'a>> = node
            .descendants()
            .filter(|d| d.is(NodeType::If) && d.is_ternary())
            .collect();
        for nested_ternary in nested {
            let ignored = &mut self.ignored;
            cx.add_offense_with_correction(nested_ternary.source_range(), MSG, |corrector| {
                if ignored.part_of(node) {
                    return;
                }
                autocorrect(src, corrector, node);
                ignored.ignore(node);
            });
        }
    }
}

fn autocorrect(src: &str, corrector: &mut Corrector, if_node: Node<'_>) {
    replace_loc_and_whitespace(src, corrector, if_node.loc().question.unwrap(), "\n");
    replace_loc_and_whitespace(src, corrector, if_node.loc().colon.unwrap(), "\nelse\n");
    let if_branch = if_node.if_branch().unwrap();
    corrector.replace(if_branch.source_range(), remove_parentheses(if_branch.source()));
    corrector.wrap(if_node.source_range(), "if ", "\nend");
}

fn remove_parentheses(source: &str) -> &str {
    if source.starts_with('(') && source.ends_with(')') {
        let s = source.strip_prefix('(').unwrap();
        s.strip_suffix(')').unwrap_or(s)
    } else {
        source
    }
}

fn replace_loc_and_whitespace(src: &str, corrector: &mut Corrector, range: Range, replacement: &str) {
    let opts = SurroundingSpace {
        whitespace: true,
        ..SurroundingSpace::default()
    };
    corrector.replace(range_with_surrounding_space(src, range, opts), replacement);
}
