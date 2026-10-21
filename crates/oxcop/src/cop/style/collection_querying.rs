//! Port of `lib/rubocop/cop/style/collection_querying.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static COUNT_PREDICATE: NodePattern = NodePattern::new(
    "(send
       {
         (any_block $(call !nil? :count) _ _)
         $(call !nil? :count (block-pass _)?)
       }
       {
         :positive? |
         :> (int 0) |
         :!= (int 0) |
         :zero? |
         :== (int 0) |
         :== (int 1) |
         :> (int 1)
       })",
);

#[derive(Default)]
pub struct CollectionQuerying;

impl Cop for CollectionQuerying {
    fn name(&self) -> &'static str {
        "Style/CollectionQuerying"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["positive?", ">", "!=", "zero?", "=="]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(count_node) = COUNT_PREDICATE
            .match_node(node)
            .and_then(|c| c[0].node())
        else {
            return;
        };
        let Some(replacement_method) = replacement_method(node) else {
            return;
        };
        if replacement_method == "many?"
            && !cx
                .config
                .all_cops_bool("ActiveSupportExtensionsEnabled", false)
        {
            return;
        }
        let selector = count_node.loc().selector.unwrap();
        let offense_range = selector.join(node.source_range().end());
        let message = format!("Use `{replacement_method}` instead.");
        let removal = removal_range(cx.processed_source.buffer_source(), node);
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            corrector.replace(selector, replacement_method);
            corrector.remove(removal);
        });
    }
}

fn replacement_method(node: Node<'_>) -> Option<&'static str> {
    let value = node.first_argument().and_then(|a| a.int_value());
    Some(match (node.method_name()?, value) {
        ("positive?", None) | (">", Some(0)) | ("!=", Some(0)) => "any?",
        ("zero?", None) | ("==", Some(0)) => "none?",
        ("==", Some(1)) => "one?",
        (">", Some(1)) => "many?",
        _ => return None,
    })
}

fn removal_range(src: &str, node: Node<'_>) -> Range {
    let range = node
        .loc()
        .dot
        .or(node.loc().selector)
        .unwrap()
        .join(node.source_range().end());
    range_with_surrounding_space(src, range, SurroundingSpace::side(Side::Left))
}
