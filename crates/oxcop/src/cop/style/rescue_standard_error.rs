//! Port of `lib/rubocop/cop/style/rescue_standard_error.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::rescue_node::{modifier_locations, rescue_modifier};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG_IMPLICIT: &str = "Omit the error class when rescuing `StandardError` by itself.";
const MSG_EXPLICIT: &str = "Avoid rescuing without specifying an error class.";

static RESCUE_WITHOUT_ERROR_CLASS: NodePattern = NodePattern::new("(resbody nil? _ _)");
static RESCUE_STANDARD_ERROR: NodePattern =
    NodePattern::new("(resbody $(array (const {nil? cbase} :StandardError)) _ _)");

#[derive(Default)]
pub struct RescueStandardError {
    modifier_locations: Option<Vec<Range>>,
}

impl Cop for RescueStandardError {
    fn name(&self) -> &'static str {
        "Style/RescueStandardError"
    }

    fn support_autocorrect(&self) -> bool {
        true
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
        if rescue_modifier(locations, node) {
            return;
        }
        let keyword = node.loc().keyword.unwrap();
        match style(cx) {
            "implicit" => {
                let Some(c) = RESCUE_STANDARD_ERROR.match_node(node) else {
                    return;
                };
                let error = c[0].node().unwrap();
                let range = keyword.join(error.source_range());
                cx.add_offense_with_correction(range, MSG_IMPLICIT, |corrector| {
                    corrector.remove(range_between(keyword.end_pos, error.source_range().end_pos));
                });
            }
            "explicit" => {
                if !RESCUE_WITHOUT_ERROR_CLASS.is_match(node) {
                    return;
                }
                cx.add_offense_with_correction(keyword, MSG_EXPLICIT, |corrector| {
                    corrector.insert_after(keyword, " StandardError");
                });
            }
            _ => {}
        }
    }
}
