//! Port of `lib/rubocop/cop/style/trailing_underscore_variable.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::parentheses;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct TrailingUnderscoreVariable;

impl Cop for TrailingUnderscoreVariable {
    fn name(&self) -> &'static str {
        "Style/TrailingUnderscoreVariable"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Masgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let allow_named = cx.cop_config_bool("AllowNamedUnderscoreVariables", false);
        for range in unneeded_ranges(allow_named, node) {
            let mut good_code = node.source().to_owned();
            let offset = range.begin_pos - node.source_range().begin_pos;
            good_code.replace_range(offset..offset + range.len(), "");
            let message = format!(
                "Do not use trailing `_`s in parallel assignment. Prefer `{good_code}`."
            );
            cx.add_offense_with_correction(range, &message, |corrector| {
                corrector.remove(range);
            });
        }
    }
}

fn find_first_offense<'a>(allow_named: bool, variables: &[Node<'a>]) -> Option<Node<'a>> {
    let first_offense = find_first_possible_offense(allow_named, variables)?;
    if splat_variable_before(first_offense, variables) {
        return None;
    }
    Some(first_offense)
}

/// Walks the variables from the right while they are underscore variables.
fn find_first_possible_offense<'a>(allow_named: bool, variables: &[Node<'a>]) -> Option<Node<'a>> {
    let mut offense = None;
    for variable in variables.iter().rev() {
        if !variable.is_any(&[NodeType::Lvasgn, NodeType::Splat]) {
            break;
        }
        // `var, = *variable; var, = *var`.
        let var: Option<String> = match variable.children().first() {
            Some(Child::Sym(name)) => Some(name.clone()),
            Some(Child::Node(id)) => variable
                .ast()
                .node(*id)
                .children()
                .first()
                .and_then(Child::as_sym)
                .map(str::to_owned),
            _ => None,
        };
        let var = var.unwrap_or_default();
        if (allow_named && var != "_") || !var.starts_with('_') {
            break;
        }
        offense = Some(*variable);
    }
    offense
}

fn splat_variable_before(first_offense: Node<'_>, variables: &[Node<'_>]) -> bool {
    // `reverse_index` compares with `==`, which is structural.
    let index = variables
        .iter()
        .rposition(|v| v.same_as(first_offense))
        .unwrap_or(0);
    variables[..index].iter().any(|v| v.is(NodeType::Splat))
}

fn mlhs_of(node: Node<'_>) -> Node<'_> {
    if node.is(NodeType::Masgn) {
        node.child(0).unwrap()
    } else {
        node
    }
}

fn unneeded_ranges(allow_named: bool, node: Node<'_>) -> Vec<Range> {
    let variables: Vec<Node<'_>> = mlhs_of(node).child_nodes().collect();
    let mut ranges = children_offenses(allow_named, &variables);
    if let Some(main_offense) = main_node_offense(allow_named, node) {
        ranges.push(main_offense);
    }
    ranges
}

fn main_node_offense(allow_named: bool, node: Node<'_>) -> Option<Range> {
    let mlhs_node = mlhs_of(node);
    let variables: Vec<Node<'_>> = mlhs_node.child_nodes().collect();
    let first_offense = find_first_offense(allow_named, &variables)?;
    if first_offense.source_range() == variables[0].source_range() {
        return Some(unused_range(node, mlhs_node));
    }
    if parentheses(mlhs_node) {
        return Some(range_between(
            first_offense.source_range().begin_pos - 1,
            mlhs_node.source_range().end_pos - 1,
        ));
    }
    Some(range_between(
        first_offense.source_range().begin_pos,
        node.loc().operator.unwrap().begin_pos,
    ))
}

fn children_offenses(allow_named: bool, variables: &[Node<'_>]) -> Vec<Range> {
    variables
        .iter()
        .filter(|v| v.is(NodeType::Mlhs))
        .flat_map(|v| unneeded_ranges(allow_named, *v))
        .collect()
}

fn unused_range(node: Node<'_>, mlhs_node: Node<'_>) -> Range {
    let start = mlhs_node.source_range().begin_pos;
    let end = if node.is(NodeType::Masgn) {
        node.child(1).unwrap().source_range().begin_pos
    } else {
        mlhs_node.source_range().end_pos
    };
    range_between(start, end)
}
