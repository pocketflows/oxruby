//! Port of `lib/rubocop/cop/style/multiline_method_signature.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::line_length_help::max_line_length;
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, arguments_range, range_between, range_by_whole_lines,
    range_with_surrounding_space,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::{ProcessedSource, Range};
use crate::regex;

const MSG: &str = "Avoid multi-line method signatures.";

#[derive(Default)]
pub struct MultilineMethodSignature;

impl Cop for MultilineMethodSignature {
    fn name(&self) -> &'static str {
        "Style/MultilineMethodSignature"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.has_arguments() {
            return;
        }
        let arguments = node.arguments_node().unwrap();
        if node.first_line() == arguments.last_line() {
            return;
        }
        if correction_exceeds_max_line_length(cx, node, arguments) {
            return;
        }
        let Some(begin_of_arguments) = arguments.loc().begin else {
            return;
        };
        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            autocorrect(ps, corrector, node, arguments, begin_of_arguments);
        });
    }
}

fn autocorrect(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    arguments: Node<'_>,
    begin_of_arguments: Range,
) {
    let mut joined_arguments = arguments
        .child_nodes()
        .map(|a| a.source())
        .collect::<Vec<_>>()
        .join(", ");
    let last_line_source = ps.lines()[arguments.last_line() - 1].trim();
    if last_line_source.starts_with(')') {
        joined_arguments.push_str(last_line_source);
        corrector.remove(range_by_whole_lines(ps, arguments.loc().end.unwrap(), true));
    }

    let src = ps.buffer_source();
    let arguments_range = range_with_surrounding_space(
        src,
        arguments_range(node),
        SurroundingSpace::side(Side::Left),
    );
    // If the method name isn't on the same line as `def`, pull the name and
    // the opening parenthesis up next to `def` so the collapsed signature
    // stays on a single line and remains valid Ruby.
    if ps.line_of(arguments_range.begin_pos) != node.first_line() {
        let prefix_range = range_between(
            node.loc().keyword.unwrap().end_pos,
            begin_of_arguments.begin_pos,
        );
        corrector.replace(prefix_range, format!(" {}", ps.slice(prefix_range).trim()));
    }
    corrector.remove(arguments_range);
    corrector.insert_after(begin_of_arguments, joined_arguments);
}

fn correction_exceeds_max_line_length(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    arguments: Node<'_>,
) -> bool {
    let Some(max) = max_line_length(cx) else {
        return false;
    };
    let indentation_width = cx.processed_source.line_indentation(node.first_line());
    // Measure the collapsed single-line width the autocorrect would produce.
    let signature = node.range_source(
        node.source_range()
            .begin()
            .join(arguments.source_range().end()),
    );
    let definition_width = regex!(r"\s+")
        .replace_all(signature, " ")
        .chars()
        .count();
    indentation_width + definition_width > max
}
