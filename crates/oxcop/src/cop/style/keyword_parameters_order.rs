//! Port of `lib/rubocop/cop/style/keyword_parameters_order.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, arguments_range, range_with_surrounding_comma,
    range_with_surrounding_space,
};
use crate::cop::mixin::util::parentheses;
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "Place optional keyword parameters at the end of the parameters list.";

#[derive(Default)]
pub struct KeywordParametersOrder;

impl Cop for KeywordParametersOrder {
    fn name(&self) -> &'static str {
        "Style/KeywordParametersOrder"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Kwoptarg]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let kwarg_nodes: Vec<Node<'a>> = node
            .right_siblings()
            .into_iter()
            .filter(|s| s.is(NodeType::Kwarg))
            .collect();
        if kwarg_nodes.is_empty() {
            return;
        }
        let src = cx.processed_source.buffer_source();
        cx.add_offense_cx(node.source_range(), MSG, |corrector, cx| {
            let Some(defining_node) = node
                .ancestors()
                .find(|a| a.is_any_def() || a.is(NodeType::Block))
            else {
                return;
            };
            if cx
                .processed_source
                .contains_comment(arguments_range(defining_node))
            {
                return;
            }
            let first_kwoptarg = node
                .parent()
                .and_then(|p| p.child_nodes().find(|c| c.is(NodeType::Kwoptarg)));
            if first_kwoptarg != Some(node) {
                return;
            }
            autocorrect(src, corrector, node, defining_node, &kwarg_nodes);
        });
    }
}

fn autocorrect(
    src: &str,
    corrector: &mut Corrector,
    node: Node<'_>,
    defining_node: Node<'_>,
    kwarg_nodes: &[Node<'_>],
) {
    let sources: Vec<&str> = kwarg_nodes.iter().map(|k| k.source()).collect();
    corrector.insert_before(node.source_range(), format!("{}, ", sources.join(", ")));

    let Some(arguments) = defining_node.arguments_node() else {
        return;
    };
    if !parentheses(arguments) {
        append_newline_to_last_kwoptarg(arguments, corrector);
    }
    remove_kwargs(src, kwarg_nodes, corrector);
}

/// The newline only needs restoring when the moved keyword argument was the
/// last parameter, so removing it also consumes the line break before the
/// body. When a `kwoptarg` already trails the list, the body stays separated
/// and inserting a newline would leave a spurious blank line.
fn append_newline_to_last_kwoptarg(arguments: Node<'_>, corrector: &mut Corrector) {
    if !arguments
        .child_nodes()
        .last()
        .is_some_and(|l| l.is(NodeType::Kwarg))
    {
        return;
    }
    if arguments.parent().is_some_and(|p| p.is(NodeType::Block)) {
        return;
    }
    if let Some(last_kwoptarg) = arguments
        .child_nodes()
        .rev()
        .find(|a| a.is(NodeType::Kwoptarg))
    {
        corrector.insert_after(last_kwoptarg.source_range(), "\n");
    }
}

fn remove_kwargs(src: &str, kwarg_nodes: &[Node<'_>], corrector: &mut Corrector) {
    for kwarg in kwarg_nodes {
        let with_space =
            range_with_surrounding_space(src, kwarg.source_range(), SurroundingSpace::default());
        corrector.remove(range_with_surrounding_comma(src, with_space, Side::Left));
    }
}
