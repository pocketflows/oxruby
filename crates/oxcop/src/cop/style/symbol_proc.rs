//! Port of `lib/rubocop/cop/style/symbol_proc.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::comments_help::{comments_contain_disables, contains_comments};
use crate::cop::mixin::range_help::{
    range_between, range_with_surrounding_comma, range_with_surrounding_space, Side,
    SurroundingSpace,
};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

static PROC_NODE: NodePattern = NodePattern::new("(send (const {nil? cbase} :Proc) :new)");

// `#symbol_proc_receiver?` is inlined.
static SYMBOL_PROC: NodePattern = NodePattern::new(
    "{
       (block ${(call ...) (super ...) zsuper} $(args (arg _var)) (send (lvar _var) $_))
       (numblock ${(call ...) (super ...) zsuper} $1 (send (lvar :_1) $_))
       (itblock ${(call ...) (super ...) zsuper} $_ (send (lvar :it) $_))
     }",
);

#[derive(Default)]
pub struct SymbolProc;

impl Cop for SymbolProc {
    fn name(&self) -> &'static str {
        "Style/SymbolProc"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Layout/SpaceBeforeBlockBraces"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = SYMBOL_PROC.match_node(node) else {
            return;
        };
        let dispatch_node = captures[0].node().unwrap();
        let arguments_node = captures[1].node();
        let method_name = captures[2].sym().unwrap_or_default();
        let dispatch_method = dispatch_node.method_name().unwrap_or_default();
        if cx
            .config
            .all_cops_bool("ActiveSupportExtensionsEnabled", false)
        {
            if PROC_NODE.is_match(dispatch_node) {
                return;
            }
            if matches!(dispatch_method, "lambda" | "proc") {
                return;
            }
        }
        if unsafe_hash_usage(dispatch_node) || unsafe_array_usage(dispatch_node) {
            return;
        }
        if allowed_method(cx, dispatch_method) || matches_allowed_pattern(cx, dispatch_method) {
            return;
        }
        let send_node = node.send_node().unwrap();
        if cx.cop_config_bool("AllowMethodsWithArguments", false) && send_node.has_arguments() {
            return;
        }
        if node.is(NodeType::Block)
            && arguments_node.is_some_and(|args| {
                args.children().len() == 1 && args.source().contains(',')
            })
        {
            return;
        }
        if cx.cop_config_bool("AllowComments", false)
            && contains_comments(cx.processed_source, node)
            && !comments_contain_disables(cx, node, cx.cop_name())
        {
            return;
        }
        let range = range_between(
            node.loc().begin.unwrap().begin_pos,
            node.loc().end.unwrap().end_pos,
        );
        let message = format!(
            "Pass `&:{method_name}` as an argument to `{dispatch_method}` instead of a block."
        );
        let src = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(range, &message, |corrector| {
            autocorrect(src, corrector, node);
        });
    }
}

fn unsafe_hash_usage(node: Node<'_>) -> bool {
    node.receiver().is_some_and(|r| r.is(NodeType::Hash))
        && matches!(node.method_name(), Some("reject" | "select"))
}

fn unsafe_array_usage(node: Node<'_>) -> bool {
    node.receiver().is_some_and(|r| r.is(NodeType::Array))
        && matches!(node.method_name(), Some("min" | "max"))
}

fn autocorrect(src: &str, corrector: &mut Corrector, node: Node<'_>) {
    let send_node = node.send_node().unwrap();
    let body_method = node.body().unwrap().method_name().unwrap_or_default();
    if send_node.has_arguments() {
        autocorrect_with_args(src, corrector, node, &send_node.arguments(), body_method);
    } else {
        autocorrect_without_args(src, corrector, node, body_method);
    }
}

fn autocorrect_without_args(src: &str, corrector: &mut Corrector, node: Node<'_>, method: &str) {
    let send_node = node.send_node().unwrap();
    if send_node.is_lambda_literal() {
        if send_node.range_source(send_node.loc().selector.unwrap()) == "->" {
            corrector.replace(node.source_range(), format!("lambda(&:{method})"));
            return;
        }
        autocorrect_lambda_block(corrector, node);
    }
    corrector.replace(block_range_with_space(src, node), format!("(&:{method})"));
}

fn autocorrect_with_args(
    src: &str,
    corrector: &mut Corrector,
    node: Node<'_>,
    args: &[Node<'_>],
    method_name: &str,
) {
    let arg_range = args.last().unwrap().source_range();
    let arg_range = range_with_surrounding_comma(src, arg_range, Side::Right);
    let mut replacement = format!(" &:{method_name}");
    if !src[arg_range.begin_pos..arg_range.end_pos].ends_with(',') {
        replacement = format!(",{replacement}");
    }
    corrector.insert_after(arg_range, replacement);
    corrector.remove(block_range_with_space(src, node));
}

fn autocorrect_lambda_block(corrector: &mut Corrector, node: Node<'_>) {
    let send_node = node.send_node().unwrap();
    let selector = send_node.loc().selector.unwrap();
    corrector.replace(selector, "lambda");
    let range = range_between(selector.end_pos, node.loc().begin.unwrap().end_pos - 2);
    corrector.remove(range);
}

fn block_range_with_space(src: &str, node: Node<'_>) -> Range {
    let block_range = range_between(begin_pos_for_replacement(node), node.loc().end.unwrap().end_pos);
    range_with_surrounding_space(src, block_range, SurroundingSpace::side(Side::Left))
}

fn begin_pos_for_replacement(node: Node<'_>) -> usize {
    let send_node = node.send_node().unwrap();
    if send_node.is_parenthesized() && send_node.arguments().is_empty() {
        send_node.loc().begin.unwrap().begin_pos
    } else {
        node.loc().begin.unwrap().begin_pos
    }
}
