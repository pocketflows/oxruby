//! Port of `lib/rubocop/cop/lint/ambiguous_regexp_literal.rb`.
//!
//! RuboCop reads `diagnostic.reason == :ambiguous_regexp`, which
//! `Prism::Translation::Parser` makes from Prism's `ambiguous_slash`
//! warning; the Rust Prism API only exposes the warning's message. Prism
//! never produces `:ambiguous_literal` (the `parser` gem's reason before
//! Ruby 3.0), so nothing is reported for older target versions, as in
//! RuboCop under Prism.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::add_parentheses;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Ambiguous regexp literal. Parenthesize the method arguments if it's surely a regexp literal, or add a whitespace to the right of the `/` if it should be a division.";

const AMBIGUOUS_SLASH: &str =
    "ambiguous `/`; wrap regexp in parentheses or add a space after `/` operator";

#[derive(Default)]
pub struct AmbiguousRegexpLiteral;

impl Cop for AmbiguousRegexpLiteral {
    fn name(&self) -> &'static str {
        "Lint/AmbiguousRegexpLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        if cx.target_ruby_version() < 3.0 {
            return;
        }
        let ps = cx.processed_source;
        let locations: Vec<Range> = ps
            .parse_result()
            .warnings()
            .filter(|warning| warning.message() == AMBIGUOUS_SLASH.as_bytes())
            .map(|warning| Range::from(&warning.location()))
            .collect();
        let Some(ast) = ps.ast() else {
            return;
        };
        for location in locations {
            let offense_node = find_offense_node_by(ast, location);
            let src = ps.buffer_source();
            cx.add_offense_with_correction(location, MSG, |corrector| {
                if let Some(offense_node) = offense_node {
                    add_parentheses(src, offense_node, corrector);
                }
            });
        }
    }
}

fn find_offense_node_by<'a>(ast: Node<'a>, location: Range) -> Option<Node<'a>> {
    let node = ast.each_node().find(|regexp_node| {
        regexp_node.is(NodeType::Regexp)
            && regexp_node.source_range().begin_pos == location.begin_pos
    })?;
    Some(find_offense_node(node.parent()?, node))
}

fn find_offense_node<'a>(node: Node<'a>, regexp_receiver: Node<'a>) -> Node<'a> {
    let Some(parent) = node.parent() else {
        return node;
    };
    if first_argument_is_regexp(node) {
        return node;
    }
    if (parent.is_send() && node.receiver().is_some())
        || method_chain_to_regexp_receiver(node, regexp_receiver)
    {
        return find_offense_node(parent, regexp_receiver);
    }
    node
}

fn first_argument_is_regexp(node: Node<'_>) -> bool {
    node.is_send() && node.first_argument().is_some_and(|a| a.is(NodeType::Regexp))
}

fn method_chain_to_regexp_receiver(node: Node<'_>, regexp_receiver: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let Some(parent_receiver) = parent.receiver() else {
        return false;
    };
    parent.parent().is_some()
        && parent_receiver
            .receiver()
            .is_some_and(|r| r.same_as(regexp_receiver))
}
