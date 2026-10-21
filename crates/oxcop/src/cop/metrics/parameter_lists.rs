//! Port of `lib/rubocop/cop/metrics/parameter_lists.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static STRUCT_NEW_OR_DATA_DEFINE_BLOCK: NodePattern = NodePattern::new(
    "(block
       {
         (send (const {nil? cbase} :Struct) :new ...)
         (send (const {nil? cbase} :Data) :define ...)
       }
       (args) ...)",
);

#[derive(Default)]
pub struct ParameterLists;

impl Cop for ParameterLists {
    fn name(&self) -> &'static str {
        "Metrics/ParameterLists"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs, NodeType::Args]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Args) {
            on_args(cx, node);
            return;
        }
        let optargs = node
            .arguments()
            .iter()
            .filter(|a| a.is(NodeType::Optarg))
            .count() as i64;
        let max = cx.cop_config_int("MaxOptionalParameters").unwrap_or(0);
        if optargs <= max {
            return;
        }
        let message = format!("Method has too many optional parameters. [{optargs}/{max}]");
        cx.add_offense_yield(node.source_range(), &message, |cx| {
            cx.set_exclude_limit("MaxOptionalParameters", optargs);
        });
    }
}

fn on_args<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(parent) = node.parent() else {
        return;
    };
    if parent.is_method("initialize")
        && parent
            .parent()
            .is_some_and(|p| STRUCT_NEW_OR_DATA_DEFINE_BLOCK.is_match(p))
    {
        return;
    }
    let count = args_count(cx, node);
    let max = cx.cop_config_int("Max").unwrap_or(0);
    if count <= max {
        return;
    }
    // `^lambda_or_proc?`
    if parent.is_lambda_or_proc() {
        return;
    }
    let message =
        format!("Avoid parameter lists longer than {max} parameters. [{count}/{max}]");
    cx.add_offense_yield(node.source_range(), &message, |cx| {
        cx.set_exclude_limit("Max", count);
    });
}

fn args_count(cx: &Investigation<'_, '_>, node: Node<'_>) -> i64 {
    let count_keyword_args = cx.cop_config_bool("CountKeywordArgs", false);
    node.child_nodes()
        .filter(|a| {
            !a.is(NodeType::Blockarg)
                && (count_keyword_args || !matches!(a.ty(), NodeType::Kwoptarg | NodeType::Kwarg))
        })
        .count() as i64
}
