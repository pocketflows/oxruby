//! Port of `lib/rubocop/cop/style/rescue_modifier.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::parentheses_corrector;
use crate::cop::mixin::alignment::{indentation, offset};
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::rescue_node::{modifier_locations, rescue_modifier};
use crate::cop::mixin::util::parentheses;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Avoid using `rescue` in its modifier form.";

#[derive(Default)]
pub struct RescueModifier {
    modifier_locations: Option<Vec<Range>>,
}

impl Cop for RescueModifier {
    fn name(&self) -> &'static str {
        "Style/RescueModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/MethodCallWithArgsParentheses"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Resbody]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.modifier_locations = None;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let locations = self
            .modifier_locations
            .get_or_insert_with(|| modifier_locations(cx.processed_source));
        if !rescue_modifier(locations, node) {
            return;
        }
        let rescue_node = node.parent().unwrap();
        let parenthesized = rescue_node.parent().is_some_and(parentheses);
        let mut node_indentation = indentation(cx, rescue_node);
        let mut node_offset = offset(rescue_node);
        if parenthesized {
            node_indentation.pop();
            node_offset.pop();
        }
        cx.add_offense_with_correction(rescue_node.source_range(), MSG, |corrector| {
            let operation = rescue_node.body().unwrap();
            if operation.is(NodeType::Array) && !operation.is_bracketed() {
                corrector.wrap(operation.source_range(), "[", "]");
            }
            corrector.remove(range_between(
                operation.source_range().end_pos,
                rescue_node.source_range().end_pos,
            ));
            corrector.insert_before(
                operation.source_range(),
                format!("begin\n{node_indentation}"),
            );
            let rescue_body = rescue_node.resbody_branches()[0]
                .body()
                .map_or("", |b| b.source());
            corrector.insert_after(
                heredoc_end(operation).unwrap_or(operation.source_range()),
                format!("\n{node_offset}rescue\n{node_indentation}{rescue_body}\n{node_offset}end"),
            );
            if parenthesized {
                parentheses_corrector::correct(corrector, rescue_node.parent().unwrap());
            }
        });
    }
}

fn heredoc_end(node: Node<'_>) -> Option<Range> {
    if !node.is_call() {
        return None;
    }
    let heredoc = node
        .arguments()
        .into_iter()
        .rev()
        .find(|a| a.is_any_str() && a.is_heredoc())?;
    heredoc.loc().heredoc_end
}
