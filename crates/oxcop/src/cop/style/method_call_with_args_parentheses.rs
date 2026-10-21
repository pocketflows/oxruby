//! Port of `lib/rubocop/cop/style/method_call_with_args_parentheses.rb` and
//! its `method_call_with_args_parentheses/require_parentheses.rb` and
//! `omit_parentheses.rb` modules.

use crate::ast::{Ast, Node, NodeId, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::mixin::reparsed_equivalence::{Oversized, ReparsedEquivalence, verified_by_reparse};
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;
use crate::regex;

const REQUIRE_MSG: &str = "Use parentheses for method calls with arguments.";
const OMIT_MSG: &str = "Omit parentheses for method calls with arguments.";

#[derive(Default)]
pub struct MethodCallWithArgsParentheses {
    /// `@pending_omit_offenses`.
    pending_omit_offenses: Vec<NodeId>,
}

impl Cop for MethodCallWithArgsParentheses {
    fn name(&self) -> &'static str {
        "Style/MethodCallWithArgsParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &[
            "Style/NestedParenthesizedCalls",
            "Style/RescueModifier",
            "Style/TrailingCommaInArguments",
        ]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend, NodeType::Yield]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.pending_omit_offenses.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if style(cx) == "omit_parentheses" {
            if omit_parentheses(cx, node) {
                self.pending_omit_offenses.push(node.id());
            }
        } else {
            require_parentheses(cx, node);
        }
    }

    fn on_investigation_end(&mut self, cx: &mut Investigation<'_, '_>) {
        let pending = std::mem::take(&mut self.pending_omit_offenses);
        if pending.is_empty() {
            return;
        }
        let ast = cx.processed_source.tree();
        let verifier = OmitVerifier { ast };
        for id in verified_by_reparse(&verifier, cx, &pending, Oversized::Report) {
            let node = ast.node(id);
            let range = node.loc().begin.unwrap().join(node.loc().end.unwrap());
            let src = cx.processed_source.buffer_source();
            cx.add_offense_with_correction(range, OMIT_MSG, |corrector| {
                autocorrect(src, corrector, node);
            });
        }
    }
}

struct OmitVerifier<'a> {
    ast: &'a Ast,
}

impl ReparsedEquivalence<NodeId> for OmitVerifier<'_> {
    fn apply_reparse_correction(
        &self,
        cx: &Investigation<'_, '_>,
        corrector: &mut Corrector,
        item: NodeId,
    ) {
        autocorrect(
            cx.processed_source.buffer_source(),
            corrector,
            self.ast.node(item),
        );
    }

    fn item_range(&self, item: NodeId) -> Range {
        self.ast.node(item).source_range()
    }
}

fn args_begin(node: Node<'_>) -> Range {
    let loc = node.loc();
    let selector = if node.is(NodeType::Yield) {
        loc.keyword
    } else {
        loc.selector
    }
    .unwrap();
    let resize_by = if args_parenthesized(node) { 2 } else { 1 };
    selector.end().resize(resize_by)
}

fn args_end(node: Node<'_>) -> Range {
    node.source_range().end()
}

fn args_parenthesized(node: Node<'_>) -> bool {
    let arguments = node.arguments();
    if arguments.len() != 1 {
        return false;
    }
    let first_node = arguments[0];
    first_node.is(NodeType::Begin) && first_node.is_parenthesized_call()
}

fn is_assignment(node: Node<'_>) -> bool {
    if node.is_call() {
        node.is_setter_method()
    } else {
        node.is_assignment()
    }
}

// ---- RequireParentheses ------------------------------------------------------

fn require_parentheses(cx: &mut Investigation<'_, '_>, node: Node<'_>) {
    let name = node.method_name().unwrap_or("");
    if allowed_method(cx, name) || matches_allowed_pattern(cx, name) {
        return;
    }
    if eligible_for_parentheses_omission(cx, node) {
        return;
    }
    if !node.has_arguments() || node.is_parenthesized() {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), REQUIRE_MSG, |corrector| {
        corrector.replace(args_begin(node), "(");
        if !args_parenthesized(node) {
            corrector.insert_after(args_end(node), ")");
        }
    });
}

fn eligible_for_parentheses_omission(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is_operator_method() || node.is_setter_method() || ignored_macro(cx, node)
}

fn matches_included_macro_pattern(cx: &Investigation<'_, '_>, method_name: &str) -> bool {
    cx.cop_config_list("IncludedMacroPatterns")
        .iter()
        .filter_map(|p| ruby_regex(p))
        .any(|re| re.find(method_name).is_some())
}

fn ignored_macro(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let name = node.method_name().unwrap_or("");
    cx.cop_config_bool("IgnoreMacros", false)
        && node.is_macro()
        && !cx.cop_config_list("IncludedMacros").contains(&name)
        && !matches_included_macro_pattern(cx, name)
}

// ---- OmitParentheses ---------------------------------------------------------

/// `omit_parentheses(node)`: whether the node goes to the pending offenses.
fn omit_parentheses(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is_parenthesized()
        && !inside_endless_method_def(node)
        && !require_parentheses_for_hash_value_omission(node)
        && !syntax_like_method_call(node)
        && !method_call_before_constant_resolution(node)
        && !super_call_without_arguments(node)
        && !legitimate_call_with_parentheses(cx, node)
        && !allowed_camel_case_method_call(cx, node)
        && !allowed_string_interpolation_method_call(cx, node)
}

fn autocorrect(src: &str, corrector: &mut Corrector, node: Node<'_>) {
    let range = args_begin(node);
    if parentheses_at_the_end_of_multiline_call(node) {
        let with_whitespace = range_with_surrounding_space(
            src,
            range,
            SurroundingSpace {
                side: Side::Right,
                newlines: false,
                ..SurroundingSpace::default()
            },
        );
        corrector.replace(with_whitespace, " \\");
    } else {
        corrector.replace(range, " ");
    }
    corrector.remove(node.loc().end.unwrap());
}

fn inside_endless_method_def(node: Node<'_>) -> bool {
    node.ancestors().any(|a| a.is_any_def() && a.is_endless()) && node.has_arguments()
}

fn require_parentheses_for_hash_value_omission(node: Node<'_>) -> bool {
    let Some(last_argument) = node.last_argument() else {
        return false;
    };
    if !last_argument.is(NodeType::Hash)
        || !last_argument
            .pairs()
            .last()
            .is_some_and(|p| p.is_value_omission())
    {
        return false;
    }
    node.parent()
        .is_some_and(|p| p.is_conditional() || p.is_single_line())
        || !last_expression(node)
}

fn last_expression(node: Node<'_>) -> bool {
    match node.parent() {
        Some(parent) if is_assignment(parent) => parent.right_sibling().is_none(),
        _ => node.right_sibling().is_none(),
    }
}

fn syntax_like_method_call(node: Node<'_>) -> bool {
    node.is_implicit_call() || node.is_operator_method()
}

fn method_call_before_constant_resolution(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is(NodeType::Const))
}

fn super_call_without_arguments(node: Node<'_>) -> bool {
    node.is(NodeType::Super) && !node.has_arguments()
}

fn allowed_camel_case_method_call(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    node.is_camel_case_method()
        && (!node.has_arguments() || cx.cop_config_bool("AllowParenthesesInCamelCaseMethod", false))
}

fn allowed_string_interpolation_method_call(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowParenthesesInStringInterpolation", false)
        && inside_string_interpolation(node)
}

fn parentheses_at_the_end_of_multiline_call(node: Node<'_>) -> bool {
    if !node.is_multiline() {
        return false;
    }
    let begin = node.loc().begin.unwrap();
    let source = node.ast().source();
    let line_start = source[..begin.begin_pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[begin.begin_pos..]
        .find('\n')
        .map_or(source.len(), |i| begin.begin_pos + i);
    let source_line = &source[line_start..line_end];
    regex!(r"\s+\Z")
        .replace(source_line, "")
        .ends_with('(')
}

fn legitimate_call_with_parentheses(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    call_in_literals(node)
        || node.parent().is_some_and(|p| p.is(NodeType::When))
        || call_with_ambiguous_arguments(node)
        || call_in_logical_operators(node)
        || call_in_optional_arguments(node)
        || call_in_single_line_inheritance(node)
        || allowed_multiline_call_with_parentheses(cx, node)
        || allowed_chained_call_with_parentheses(cx, node)
        || assignment_in_condition(node)
        || forwards_anonymous_rest_arguments(node)
}

fn parent_skipping_block(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if parent.is_any_block() {
        parent.parent()
    } else {
        Some(parent)
    }
}

fn call_in_literals(node: Node<'_>) -> bool {
    let Some(parent) = parent_skipping_block(node) else {
        return false;
    };
    parent.is_any(&[NodeType::Pair, NodeType::Array])
        || parent.is_range()
        || splat(parent)
        || ternary_if(parent)
}

fn call_in_logical_operators(node: Node<'_>) -> bool {
    let Some(parent) = parent_skipping_block(node) else {
        return false;
    };
    logical_operator(parent)
        || (parent.is(NodeType::Send) && parent.arguments().into_iter().any(logical_operator))
}

fn call_in_optional_arguments(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is_any(&[NodeType::Optarg, NodeType::Kwoptarg]))
}

fn call_in_single_line_inheritance(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::Class) && p.is_single_line())
}

fn call_with_ambiguous_arguments(node: Node<'_>) -> bool {
    call_with_braced_block(node)
        || call_in_argument_with_block(node)
        || call_as_argument_or_chain(node)
        || call_in_match_pattern(node)
        || hash_literal_in_arguments(node)
        || ambiguous_range_argument(node)
        || node.descendants().any(|n| {
            n.is(NodeType::ForwardedArgs)
                || n.is_any_block()
                || ambiguous_literal(n)
                || logical_operator(n)
        })
}

fn call_with_braced_block(node: Node<'_>) -> bool {
    (node.is_call() || node.is(NodeType::Super))
        && node.block_node().is_some_and(|b| b.has_braces())
}

fn call_in_argument_with_block(node: Node<'_>) -> bool {
    let Some(parent) = node
        .parent()
        .filter(|p| p.is_any_block())
        .and_then(|p| p.parent())
    else {
        return false;
    };
    parent.is_call() || parent.is_any(&[NodeType::Super, NodeType::Yield])
}

fn call_as_argument_or_chain(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        (parent.is_call() || parent.is_any(&[NodeType::Super, NodeType::Yield]))
            && !assigned_before(parent, node)
    })
}

fn call_in_match_pattern(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is_any_match_pattern())
}

fn hash_literal_in_arguments(node: Node<'_>) -> bool {
    node.arguments().iter().any(|n| {
        hash_literal(*n) || (n.is(NodeType::Send) && node.descendants().any(hash_literal))
    })
}

fn ambiguous_range_argument(node: Node<'_>) -> bool {
    if node
        .first_argument()
        .is_some_and(|a| a.is_range() && a.child(0).is_none())
    {
        return true;
    }
    node.last_argument()
        .is_some_and(|a| a.is_range() && a.child(1).is_none())
}

fn allowed_multiline_call_with_parentheses(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    cx.cop_config_bool("AllowParenthesesInMultilineCall", false) && node.is_multiline()
}

fn allowed_chained_call_with_parentheses(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if !cx.cop_config_bool("AllowParenthesesInChaining", false) {
        return false;
    }
    let Some(previous) = node.descendants().next().filter(|p| p.is(NodeType::Send)) else {
        return false;
    };
    previous.is_parenthesized() || allowed_chained_call_with_parentheses(cx, previous)
}

fn ambiguous_literal(node: Node<'_>) -> bool {
    splat(node) || ternary_if(node) || regexp_slash_literal(node) || unary_literal(node)
}

fn splat(node: Node<'_>) -> bool {
    node.is_any(&[NodeType::Splat, NodeType::Kwsplat, NodeType::BlockPass])
}

fn ternary_if(node: Node<'_>) -> bool {
    node.is(NodeType::If) && node.is_ternary()
}

fn logical_operator(node: Node<'_>) -> bool {
    node.is_operator_keyword() && node.is_logical_operator()
}

fn hash_literal(node: Node<'_>) -> bool {
    node.is(NodeType::Hash) && node.has_braces()
}

fn regexp_slash_literal(node: Node<'_>) -> bool {
    node.is(NodeType::Regexp) && node.loc().begin.is_some_and(|b| node.range_source(b) == "/")
}

fn unary_literal(node: Node<'_>) -> bool {
    if node.is_numeric() && (node.source().starts_with('+') || node.source().starts_with('-')) {
        return true;
    }
    node.parent()
        .is_some_and(|p| p.is(NodeType::Send) && p.is_unary_operation())
}

fn assigned_before(node: Node<'_>, target: Node<'_>) -> bool {
    is_assignment(node)
        && match (node.loc().operator, target.loc().begin) {
            (Some(operator), Some(begin)) => operator.begin_pos < begin.begin_pos,
            _ => false,
        }
}

fn inside_string_interpolation(node: Node<'_>) -> bool {
    node.ancestors()
        .skip_while(|a| !a.is(NodeType::Begin))
        .any(|a| a.is(NodeType::Dstr))
}

fn assignment_in_condition(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let Some(grandparent) = parent.parent() else {
        return false;
    };
    is_assignment(parent) && (grandparent.is_conditional() || grandparent.is(NodeType::When))
}

fn forwards_anonymous_rest_arguments(node: Node<'_>) -> bool {
    let Some(last_argument) = node.last_argument() else {
        return false;
    };
    if last_argument.is(NodeType::ForwardedRestarg) {
        return true;
    }
    last_argument.is(NodeType::Hash)
        && last_argument
            .child_nodes()
            .any(|c| c.is(NodeType::ForwardedKwrestarg))
}
