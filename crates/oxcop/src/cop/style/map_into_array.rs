//! Port of `lib/rubocop/cop/style/map_into_array.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeId, NodeType};
use crate::cop::mixin::range_help::{
    Side, SurroundingSpace, range_between, range_with_surrounding_space,
};
use crate::cop::variable_force::{Scope, VariableTable};
use crate::cop::{Cop, Corrector, Investigation};

static SUITABLE_ARGUMENT_NODE: NodePattern = NodePattern::new(
    "!{splat forwarded-restarg forwarded-args (hash (forwarded-kwrestarg)) block-pass}",
);
static EACH_BLOCK_WITH_PUSH: NodePattern = NodePattern::new(
    "[
      ^({begin kwbegin block} ...)
      (any_block (send !{nil? self} :each) _
        (send (lvar _) {:<< :push :append} #suitable_argument_node?))
    ]",
);
static EMPTY_ARRAY_ASGN: NodePattern = NodePattern::new(
    "(
      lvasgn _ {
        (array)
        (send (const {nil? cbase} :Array) :[])
        (send (const {nil? cbase} :Array) :new (array)?)
        (send nil? :Array (array))
      }
    )",
);
static EMPTY_ARRAY_TAP: NodePattern = NodePattern::new(
    "^^$(
      block
        (send (array) :tap)
        (args (arg _))
        ...
    )",
);

/// What `after_leaving_scope` keeps of a variable (`@scopes`), by node id.
struct VariableData {
    name: String,
    declaration_node: NodeId,
    references: Vec<NodeId>,
    assignments: Vec<NodeId>,
}

#[derive(Default)]
pub struct MapIntoArray {
    /// `@scopes`: each scope's variables, in `variables` order.
    scopes: Vec<Vec<VariableData>>,
}

impl Cop for MapIntoArray {
    fn name(&self) -> &'static str {
        "Style/MapIntoArray"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn joins_variable_force(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.scopes.clear();
    }

    fn after_leaving_scope<'a>(
        &mut self,
        _cx: &mut Investigation<'a, '_>,
        scope: &Scope<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        let variables = variable_table
            .scope_variables_of(scope)
            .map(|v| VariableData {
                name: v.name.to_owned(),
                declaration_node: v.declaration_node.id(),
                references: v.references.iter().map(|r| r.node.id()).collect(),
                assignments: v.assignments.iter().map(|a| a.node.id()).collect(),
            })
            .collect();
        self.scopes.push(variables);
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !each_block_with_push(node) {
            return;
        }
        let Some(dest_var) = self.find_dest_var(node) else {
            return;
        };
        let ast = node.ast();
        let declaration = ast.node(dest_var.declaration_node);
        let asgn = if offending_empty_array_tap(node, declaration) {
            declaration
        } else {
            let Some(asgn) = find_closest_assignment(node, dest_var) else {
                return;
            };
            if !EMPTY_ARRAY_ASGN.is_match(asgn) {
                return;
            }
            if !dest_used_only_for_mapping(node, dest_var, asgn) {
                return;
            }
            asgn
        };
        register_offense(cx, node, &dest_var.name, declaration, asgn);
    }
}

fn each_block_with_push(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "suitable_argument_node?"
            && val.node().is_some_and(|n| SUITABLE_ARGUMENT_NODE.is_match(n))
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    EACH_BLOCK_WITH_PUSH.match_with(node, &ctx).is_some()
}

impl MapIntoArray {
    fn find_dest_var(&self, block: Node<'_>) -> Option<&VariableData> {
        let node = block.body()?.receiver()?;
        let name = node.child_sym(0)?;
        self.scopes
            .iter()
            .filter_map(|s| s.iter().find(|v| v.name == name))
            .find(|v| v.references.contains(&node.id()))
    }
}

fn empty_array_tap(declaration_node: Node<'_>) -> Option<Node<'_>> {
    EMPTY_ARRAY_TAP
        .match_node(declaration_node)
        .and_then(|c| c[0].node())
}

/// A `tap` block only offends if the array push is the only thing in it.
fn offending_empty_array_tap(node: Node<'_>, declaration: Node<'_>) -> bool {
    empty_array_tap(declaration)
        .and_then(|tap| tap.body())
        .is_some_and(|body| body.same_as(node))
}

fn find_closest_assignment<'a>(block: Node<'a>, dest_var: &VariableData) -> Option<Node<'a>> {
    let ast = block.ast();
    dest_var
        .assignments
        .iter()
        .rev()
        .map(|&id| ast.node(id))
        .find(|node| node.source_range().end_pos < block.source_range().begin_pos)
}

fn dest_used_only_for_mapping(block: Node<'_>, dest_var: &VariableData, asgn: Node<'_>) -> bool {
    let ast = block.ast();
    let range = asgn.source_range().join(block.source_range());
    let count_in = |ids: &[NodeId]| {
        ids.iter()
            .filter(|&&id| range.contains(ast.node(id).source_range()))
            .count()
    };
    asgn.parent() == block.parent()
        && count_in(&dest_var.references) == 1
        && count_in(&dest_var.assignments) == 1
}

fn new_method_name(cx: &Investigation<'_, '_>) -> String {
    cx.config
        .for_cop("Style/CollectionMethods")
        .get("PreferredMethods")
        .and_then(|m| m.get("map"))
        .and_then(|v| v.as_str())
        .unwrap_or("map")
        .to_owned()
}

fn register_offense(
    cx: &mut Investigation<'_, '_>,
    block: Node<'_>,
    dest_var_name: &str,
    declaration: Node<'_>,
    asgn: Node<'_>,
) {
    let new_method_name = new_method_name(cx);
    let message =
        format!("Use `{new_method_name}` instead of `each` to map elements into an array.");
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(block.source_range(), &message, |corrector| {
        if return_value_used(block) {
            return;
        }
        corrector.replace(
            block.send_node().unwrap().loc().selector.unwrap(),
            &new_method_name,
        );
        if let Some(tap_block_node) = empty_array_tap(declaration) {
            remove_tap(src, corrector, block, tap_block_node);
        } else {
            remove_assignment(src, corrector, asgn);
        }
        correct_push_node(corrector, block.body().unwrap());
        correct_return_value_handling(src, corrector, block, dest_var_name);
    });
}

fn return_value_used(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.ty() {
        NodeType::Begin | NodeType::Kwbegin => {
            node.right_sibling().is_none() && return_value_used(parent)
        }
        NodeType::Def
        | NodeType::Defs
        | NodeType::Block
        | NodeType::Numblock
        | NodeType::Itblock
        | NodeType::For
        | NodeType::Ensure => !parent.is_void_context(),
        _ => true,
    }
}

fn remove_assignment(src: &str, corrector: &mut Corrector, asgn: Node<'_>) {
    let range =
        range_with_surrounding_space(src, asgn.source_range(), SurroundingSpace::side(Side::Right));
    let range = range_with_surrounding_space(
        src,
        range,
        SurroundingSpace {
            side: Side::Right,
            newlines: false,
            ..SurroundingSpace::default()
        },
    );
    corrector.remove(range);
}

fn remove_tap(src: &str, corrector: &mut Corrector, node: Node<'_>, block_node: Node<'_>) {
    corrector.remove(range_between(
        block_node.source_range().begin_pos,
        node.source_range().begin_pos,
    ));
    corrector.remove(range_with_surrounding_space(
        src,
        block_node.loc().end.unwrap(),
        SurroundingSpace::side(Side::Left),
    ));
}

fn correct_push_node(corrector: &mut Corrector, push_node: Node<'_>) {
    let arg_node = push_node.first_argument().unwrap();
    let range = push_node.source_range();
    let arg_range = arg_node.source_range();
    if arg_node.is(NodeType::Hash) && !arg_node.has_braces() {
        corrector.wrap(arg_range, "{ ", " }");
    }
    corrector.remove(range_between(range.begin_pos, arg_range.begin_pos));
    corrector.remove(range_between(arg_range.end_pos, range.end_pos));
}

fn correct_return_value_handling(
    src: &str,
    corrector: &mut Corrector,
    block: Node<'_>,
    dest_var_name: &str,
) {
    if let Some(next_node) = block.right_sibling()
        && next_node.is(NodeType::Lvar)
        && next_node.child_sym(0) == Some(dest_var_name)
    {
        corrector.remove(range_with_surrounding_space(
            src,
            next_node.source_range(),
            SurroundingSpace::side(Side::Left),
        ));
    }
    corrector.insert_before(block.source_range(), format!("{dest_var_name} = "));
}
