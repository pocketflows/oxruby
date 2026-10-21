//! Port of `lib/rubocop/cop/mixin/gemspec_help.rb`.

use crate::ast::Node;
use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::processed_source::ProcessedSource;

static GEM_SPECIFICATION: NodePattern = NodePattern::new(
    "(block
       (send
         (const
           (const {cbase nil?} :Gem) :Specification) :new)
       (args
         (arg $_)) ...)",
);

static ASSIGNMENT_METHOD_DECLARATIONS: NodePattern =
    NodePattern::new("(send (lvar {#match_block_variable_name? :_1 :it}) _ ...)");

static INDEXED_ASSIGNMENT_METHOD_DECLARATIONS: NodePattern = NodePattern::new(
    "(send
       (send (lvar {#match_block_variable_name? :_1 :it}) _)
       :[]=
       literal?
       _
     )",
);

/// `gem_specification?(node)`: the block parameter's name.
pub fn gem_specification_p(node: Node<'_>) -> Option<&str> {
    GEM_SPECIFICATION.match_node(node)?[0].sym()
}

/// `gem_specification(ast)` (`def_node_search`): the block parameter name
/// of the first `Gem::Specification.new` block.
pub fn gem_specification<'a>(ast: Option<Node<'a>>) -> Option<&'a str> {
    ast?.each_node().find_map(gem_specification_p)
}

fn search_with_block_variable<'a>(
    ps: &ProcessedSource<'_>,
    pattern: &NodePattern,
    ast: Node<'a>,
) -> Vec<Node<'a>> {
    let block_variable = gem_specification(ps.ast()).map(str::to_owned);
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        debug_assert_eq!(name, "match_block_variable_name?");
        block_variable
            .as_deref()
            .is_some_and(|b| val.sym() == Some(b))
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    pattern
        .search_with(ast, &ctx)
        .into_iter()
        .map(|(node, _)| node)
        .collect()
}

/// `assignment_method_declarations(ast)`.
pub fn assignment_method_declarations<'a>(
    ps: &ProcessedSource<'_>,
    ast: Node<'a>,
) -> Vec<Node<'a>> {
    search_with_block_variable(ps, &ASSIGNMENT_METHOD_DECLARATIONS, ast)
}

/// `indexed_assignment_method_declarations(ast)`.
pub fn indexed_assignment_method_declarations<'a>(
    ps: &ProcessedSource<'_>,
    ast: Node<'a>,
) -> Vec<Node<'a>> {
    search_with_block_variable(ps, &INDEXED_ASSIGNMENT_METHOD_DECLARATIONS, ast)
}
