//! Port of `lib/rubocop/cop/layout/space_inside_array_percent_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::match_range::each_match_range;
use crate::cop::mixin::percent_literal::process;
use crate::cop::mixin::range_help::contents_range;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Use only a single space inside array percent literal.";

#[derive(Default)]
pub struct SpaceInsideArrayPercentLiteral;

impl Cop for SpaceInsideArrayPercentLiteral {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideArrayPercentLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Array]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !process(node, &["%i", "%I", "%w", "%W"]) {
            return;
        }
        let contents = contents_range(node);
        let source = cx.source(contents);
        let regex = regex!(r"(?:[\S&&[^\\]](?:\\ )*)( {2,})(?=\S)");
        for range in each_match_range(contents, source, regex) {
            cx.add_offense_with_correction(range, MSG, |corrector| {
                corrector.replace(range, " ");
            });
        }
    }
}
