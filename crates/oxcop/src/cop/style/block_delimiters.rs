//! Port of `lib/rubocop/cop/style/block_delimiters.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{Comment, ProcessedSource, Range};
use crate::regex;

const ALWAYS_BRACES_MESSAGE: &str = "Prefer `{...}` over `do...end` for blocks.";

#[derive(Default)]
pub struct BlockDelimiters {
    ignored: IgnoredNodes,
}

impl Cop for BlockDelimiters {
    fn name(&self) -> &'static str {
        "Style/BlockDelimiters"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/RedundantBegin"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Send,
            NodeType::Csend,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_call() {
            self.on_send(node);
        } else {
            self.on_block(cx, node);
        }
    }
}

impl BlockDelimiters {
    fn on_send(&mut self, node: Node<'_>) {
        if !node.has_arguments()
            || node.is_parenthesized()
            || node.is_assignment_method()
            || single_argument_operator_method(node)
        {
            return;
        }
        for arg in node.arguments() {
            let mut blocks = Vec::new();
            get_blocks(arg, &mut blocks);
            for block in blocks {
                self.ignored.ignore(block);
            }
        }
    }

    fn on_block<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.part_of(node) || proper_block_style(cx, node) {
            return;
        }
        let message = message(cx, node);
        let ps = cx.processed_source;
        let ignored = &mut self.ignored;
        cx.add_offense_with_correction(node.loc().begin.unwrap(), &message, |corrector| {
            autocorrect(ps, corrector, node);
            ignored.ignore(node);
        });
    }
}

fn autocorrect(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if correction_would_break_code(node) {
        return;
    }
    if node.has_braces() {
        replace_braces_with_do_end(ps, corrector, node);
    } else {
        replace_do_end_with_braces(ps, corrector, node);
    }
}

fn message(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let method_name = node.method_name().unwrap_or("");
    if braces_required_method(cx, method_name) {
        return format!("Brace delimiters `{{...}}` required for '{method_name}' method.");
    }
    match style(cx) {
        "line_count_based" => if node.is_multiline() {
            "Avoid using `{...}` for multi-line blocks."
        } else {
            "Prefer `{...}` over `do...end` for single-line blocks."
        }
        .to_owned(),
        "semantic" => if node.has_braces() {
            "Prefer `do...end` over `{...}` for procedural blocks."
        } else {
            "Prefer `{...}` over `do...end` for functional blocks."
        }
        .to_owned(),
        "braces_for_chaining" => if node.is_multiline() {
            if node.is_chained() {
                "Prefer `{...}` over `do...end` for multi-line chained blocks."
            } else {
                "Prefer `do...end` for multi-line blocks without chaining."
            }
        } else {
            "Prefer `{...}` over `do...end` for single-line blocks."
        }
        .to_owned(),
        "always_braces" => ALWAYS_BRACES_MESSAGE.to_owned(),
        _ => String::new(),
    }
}

fn char_at(src: &str, pos: isize) -> &str {
    // `source[pos, 1]`; a negative position counts from the end.
    let pos = if pos < 0 {
        src.len() as isize + pos
    } else {
        pos
    };
    if pos < 0 {
        return "";
    }
    let pos = pos as usize;
    src.get(pos..)
        .and_then(|rest| rest.chars().next())
        .map_or("", |c| &src[pos..pos + c.len_utf8()])
}

fn whitespace_before(src: &str, range: Range) -> bool {
    regex!(r"\s")
        .find(char_at(src, range.begin_pos as isize - 1))
        .is_some()
}

fn whitespace_after(src: &str, range: Range, length: usize) -> bool {
    regex!(r"\s")
        .find(char_at(src, (range.begin_pos + length) as isize))
        .is_some()
}

fn replace_braces_with_do_end(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let src = ps.buffer_source();
    let b = node.loc().begin.unwrap();
    let e = node.loc().end.unwrap();
    if !whitespace_before(src, b) {
        corrector.insert_before(b, " ");
    }
    if !whitespace_before(src, e) {
        corrector.insert_before(e, " ");
    }
    if !whitespace_after(src, b, 1) {
        corrector.insert_after(b, " ");
    }
    corrector.replace(b, "do");
    if let Some(comment) = ps.comment_at_line(ps.line_of(e.begin_pos)) {
        move_comment_before_block(ps, corrector, comment, node, e);
    }
    corrector.replace(e, "end");
}

fn replace_do_end_with_braces(ps: &ProcessedSource<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let src = ps.buffer_source();
    let b = node.loc().begin.unwrap();
    let e = node.loc().end.unwrap();
    if !whitespace_after(src, b, 2) {
        corrector.insert_after(b, " ");
    }
    corrector.replace(b, "{");
    corrector.replace(e, "}");
    if begin_required(node)
        && let Some(body) = node.body()
    {
        corrector.wrap(body.source_range(), "begin\n", "\nend");
    }
}

fn move_comment_before_block(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    comment: &Comment<'_>,
    block_node: Node<'_>,
    closing_brace: Range,
) {
    let range = if block_node.is_chained() {
        end_of_chain(block_node.parent().unwrap()).source_range()
    } else {
        closing_brace
    };
    let pre_comment_range = source_range_before_comment(ps, range, comment);
    corrector.remove(range_with_surrounding_space(
        ps.buffer_source(),
        comment.range,
        SurroundingSpace::side(Side::Right),
    ));
    remove_trailing_whitespace(ps, corrector, pre_comment_range, comment);
    corrector.insert_after(pre_comment_range, "\n");
    corrector.insert_before(block_node.source_range(), format!("{}\n", comment.text));
}

fn source_range_before_comment(ps: &ProcessedSource<'_>, range: Range, comment: &Comment<'_>) -> Range {
    let range = range.end().join(comment.range.begin());
    let source = ps.slice(range);
    let trailing = source.len() - source.trim_end().len();
    if trailing > 0 {
        range.with_end(range.end_pos - trailing)
    } else {
        range
    }
}

fn end_of_chain(node: Node<'_>) -> Node<'_> {
    if node.is_call()
        && let Some(block) = node.block_node()
    {
        return end_of_chain(block);
    }
    if !node.is_chained() {
        return node;
    }
    end_of_chain(node.parent().unwrap())
}

fn remove_trailing_whitespace(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    range: Range,
    comment: &Comment<'_>,
) {
    let range_of_trailing = range.end().join(comment.range.begin());
    if regex!(r"\A\s+\z").find(ps.slice(range_of_trailing)).is_some() {
        corrector.remove(range_of_trailing);
    }
}

fn get_blocks<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    match node.ty() {
        NodeType::Block | NodeType::Numblock | NodeType::Itblock => out.push(node),
        NodeType::Send | NodeType::Csend => {
            if let Some(receiver) = node.receiver() {
                get_blocks(receiver, out);
            }
            for argument in node.arguments() {
                get_blocks(argument, out);
            }
        }
        NodeType::Hash => {
            if node.has_braces() {
                return;
            }
            for child in node.child_nodes() {
                get_blocks(child, out);
            }
        }
        NodeType::Pair => {
            for child in node.child_nodes() {
                get_blocks(child, out);
            }
        }
        _ => {}
    }
}

fn proper_block_style(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    if require_do_end(node) {
        return true;
    }
    let method_name = node.method_name().unwrap_or("");
    if allowed_method(cx, method_name) || matches_allowed_pattern(cx, method_name) {
        return true;
    }
    if braces_required_method(cx, method_name) {
        return node.has_braces();
    }
    match style(cx) {
        "line_count_based" => node.is_multiline() ^ node.has_braces(),
        "semantic" => semantic_block_style(cx, node),
        "braces_for_chaining" => {
            if node.is_multiline() {
                if node.is_chained() {
                    node.has_braces()
                } else {
                    !node.has_braces()
                }
            } else {
                node.has_braces()
            }
        }
        "always_braces" => node.has_braces(),
        _ => false,
    }
}

fn require_do_end(node: Node<'_>) -> bool {
    if node.has_braces() || node.is_multiline() {
        return false;
    }
    let Some(body) = node.body() else {
        return false;
    };
    if body.is(NodeType::Ensure) {
        return true;
    }
    if !body.is(NodeType::Rescue) {
        return false;
    }
    !modifier_rescue(body)
}

fn modifier_rescue(rescue_node: Node<'_>) -> bool {
    if rescue_node.body().is_none() || rescue_node.else_branch().is_some() {
        return false;
    }
    let resbodies = rescue_node.resbody_branches();
    if resbodies.len() != 1 {
        return false;
    }
    let resbody = resbodies[0];
    resbody.exceptions().is_empty() && resbody.exception_variable().is_none()
}

fn braces_required_method(cx: &Investigation<'_, '_>, method_name: &str) -> bool {
    cx.cop_config_list("BracesRequiredMethods")
        .contains(&method_name)
}

fn semantic_block_style(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let method_name = node.method_name().unwrap_or("");
    if node.has_braces() {
        cx.cop_config_list("FunctionalMethods").contains(&method_name)
            || return_value_used(node)
            || return_value_of_scope(node)
            || (cx.cop_config_bool("AllowBracesOnProceduralOneLiners", false)
                && node.is_single_line())
    } else {
        cx.cop_config_list("ProceduralMethods").contains(&method_name) || !return_value_used(node)
    }
}

fn correction_would_break_code(node: Node<'_>) -> bool {
    if !node.has_keywords() {
        return false;
    }
    let send_node = node.send_node().unwrap();
    send_node.has_arguments() && !send_node.is_parenthesized()
}

fn return_value_used(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.is(NodeType::Begin) {
        return_value_used(parent)
    } else {
        parent.is_assignment() || parent.is_call()
    }
}

fn return_value_of_scope(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is_conditional()
        || parent.is_operator_keyword()
        || parent.is(NodeType::Array)
        || parent.is_range()
        || matches!(parent.children().last(), Some(Child::Node(id)) if node.ast().node(*id).same_as(node))
}

fn begin_required(block_node: Node<'_>) -> bool {
    block_node
        .child_nodes()
        .any(|c| matches!(c.ty(), NodeType::Rescue | NodeType::Ensure))
        && block_node.is_multiline()
}

fn single_argument_operator_method(node: Node<'_>) -> bool {
    if !node.is_operator_method() {
        return false;
    }
    let arguments = node.arguments();
    arguments.len() == 1 && arguments[0].is(NodeType::Block)
}
