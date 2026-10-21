//! Port of `lib/rubocop/cop/style/def_with_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Omit the parentheses in defs when the method doesn't accept any arguments.";

#[derive(Default)]
pub struct DefWithParentheses;

impl Cop for DefWithParentheses {
    fn name(&self) -> &'static str {
        "Style/DefWithParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_arguments() {
            return;
        }
        let Some(arguments_range) = node.arguments_node().and_then(|a| a.loc().expression) else {
            return;
        };
        if parentheses_required(cx, node, arguments_range) {
            return;
        }
        cx.add_offense_with_correction(arguments_range, MSG, |corrector| {
            corrector.remove(arguments_range);
        });
    }
}

fn parentheses_required(cx: &Investigation<'_, '_>, node: Node<'_>, arguments_range: Range) -> bool {
    let end_pos = arguments_range.end_pos;
    let src = cx.processed_source.buffer_source();
    let token_after_argument = src
        .get(range_between(end_pos, end_pos + 1).begin_pos..(end_pos + 1).min(src.len()))
        .unwrap_or("");
    if token_after_argument == ";" {
        return false;
    }
    if node.is_single_line() && !node.is_endless() {
        return true;
    }
    token_after_argument == "="
}
