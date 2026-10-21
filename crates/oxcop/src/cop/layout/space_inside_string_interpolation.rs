//! Port of `lib/rubocop/cop/layout/space_inside_string_interpolation.rb`.

use crate::ast::token::Token;
use crate::ast::{Node, NodeType};
use crate::cop::correctors::space_corrector;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::interpolation::{NODE_TYPES, interpolations};
use crate::cop::mixin::surrounding_space::{
    SurroundingSpace, empty_brackets, no_space_offenses, space_offenses,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "{} space inside string interpolation.";

#[derive(Default)]
pub struct SpaceInsideStringInterpolation {
    ignored_nodes: IgnoredNodes,
}

impl Cop for SpaceInsideStringInterpolation {
    fn name(&self) -> &'static str {
        "Layout/SpaceInsideStringInterpolation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        for begin_node in interpolations(node) {
            self.on_interpolation(cx, begin_node);
        }
    }
}

impl SpaceInsideStringInterpolation {
    fn on_interpolation<'a>(&mut self, cx: &mut Investigation<'a, '_>, begin_node: Node<'a>) {
        if begin_node.is_multiline() {
            return;
        }
        let ps = cx.processed_source;
        let tokens = ps.tokens_within(begin_node.source_range());
        let (left, right) = delimiters(ps, begin_node);
        if empty_brackets(left, right, tokens) {
            return;
        }
        if style(cx) == "no_space" {
            no_space_offenses(self, cx, begin_node, left, right, MSG, false, false);
        } else {
            space_offenses(self, cx, begin_node, left, right, MSG, false, false);
        }
    }
}

impl SurroundingSpace for SpaceInsideStringInterpolation {
    fn autocorrect(&self, corrector: &mut Corrector, cx: &Investigation<'_, '_>, begin_node: Node<'_>) {
        let ps = cx.processed_source;
        let (Some(left), Some(right)) = delimiters(ps, begin_node) else {
            return;
        };
        if style(cx) == "no_space" {
            space_corrector::remove_space(ps, corrector, left, right);
        } else {
            space_corrector::add_space(ps, corrector, left, right);
        }
    }

    fn ignored_nodes(&mut self) -> &mut IgnoredNodes {
        &mut self.ignored_nodes
    }
}

fn delimiters<'p>(
    ps: &'p ProcessedSource<'_>,
    begin_node: Node<'_>,
) -> (Option<&'p Token>, Option<&'p Token>) {
    (
        ps.first_token_of(begin_node.source_range()),
        ps.last_token_of(begin_node.source_range()),
    )
}
