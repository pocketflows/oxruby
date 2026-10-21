//! Port of `lib/rubocop/cop/style/semicolon.rb`.

use crate::ast::lexer::Token;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::source_range;
use crate::cop::{Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Do not use semicolons to terminate expressions.";

#[derive(Default)]
pub struct Semicolon;

impl Cop for Semicolon {
    fn name(&self) -> &'static str {
        "Style/Semicolon"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/SingleLineMethods"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Begin]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let ps = cx.processed_source;
        if ps.is_blank() || !ps.raw_source().contains(';') {
            return;
        }
        check_for_line_terminator_or_opener(cx);
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if cx.cop_config_bool("AllowAsExpressionSeparator", false) || !node.source().contains(';') {
            return;
        }
        let exprs: Vec<Node<'a>> = node.child_nodes().collect();
        if exprs.len() < 2 {
            return;
        }
        // `expressions_per_line`: lines (in order) holding several expressions.
        let mut lines: Vec<(usize, usize)> = Vec::new();
        for expr in &exprs {
            let line = expr.last_line();
            match lines.iter_mut().find(|(l, _)| *l == line) {
                Some(entry) => entry.1 += 1,
                None => lines.push((line, 1)),
            }
        }
        let ps = cx.processed_source;
        for (line, count) in lines {
            if count < 2 {
                continue;
            }
            for token in ps.tokens() {
                if ps.line_of(token.range.begin_pos) == line && token.ty == "tSEMI" {
                    register_semicolon(cx, line, ps.column_of(token.range.begin_pos), true, None);
                }
            }
        }
    }
}

fn check_for_line_terminator_or_opener(cx: &mut Investigation<'_, '_>) {
    let ps = cx.processed_source;
    // `processed_source.tokens.group_by(&:line)`.
    let mut groups: Vec<(usize, Vec<&Token>)> = Vec::new();
    for token in ps.tokens() {
        let line = ps.line_of(token.range.begin_pos);
        match groups.iter_mut().find(|(l, _)| *l == line) {
            Some((_, tokens)) => tokens.push(token),
            None => groups.push((line, vec![token])),
        }
    }
    for (line, tokens) in groups {
        let Some(semicolon_pos) = semicolon_position(&tokens) else {
            continue;
        };
        let after_expr_pos = if semicolon_pos == -1 { -2 } else { semicolon_pos };
        let Some(semicolon) = at(&tokens, semicolon_pos) else {
            continue;
        };
        let column = ps.column_of(semicolon.range.begin_pos);
        let before = at(&tokens, after_expr_pos);
        register_semicolon(cx, line, column, false, before);
    }
}

/// `tokens[index]` with Ruby's negative indexing.
fn at<'t>(tokens: &[&'t Token], index: isize) -> Option<&'t Token> {
    let index = if index < 0 {
        tokens.len().checked_sub(index.unsigned_abs())?
    } else {
        index as usize
    };
    tokens.get(index).copied()
}

fn is(tokens: &[&Token], index: isize, ty: &str) -> bool {
    at(tokens, index).is_some_and(|t| t.ty == ty)
}

fn semicolon_position(tokens: &[&Token]) -> Option<isize> {
    let left_curly = |i| is(tokens, i, "tLCURLY") || is(tokens, i, "tLAMBEG");
    if is(tokens, -1, "tSEMI") {
        Some(-1)
    } else if is(tokens, 0, "tSEMI") {
        Some(0)
    } else if is(tokens, -2, "tRCURLY") && is(tokens, -3, "tSEMI") {
        Some(-3)
    } else if (left_curly(1) && is(tokens, 2, "tSEMI"))
        || (is(tokens, 1, "tSTRING_DBEG") && is(tokens, 2, "tSEMI"))
    {
        Some(2)
    } else if is(tokens, 2, "tLAMBEG") && is(tokens, 3, "tSEMI") {
        Some(3)
    } else if is(tokens, -3, "tSTRING_DEND") && is(tokens, -4, "tSEMI") {
        Some(-4)
    } else {
        None
    }
}

fn register_semicolon(
    cx: &mut Investigation<'_, '_>,
    line: usize,
    column: usize,
    after_expression: bool,
    token_before_semicolon: Option<&Token>,
) {
    let ps = cx.processed_source;
    let range = source_range(ps, line, column, 1);
    let token_before = token_before_semicolon.cloned();
    cx.add_offense_with_correction(range, MSG, |corrector| {
        if after_expression {
            if !heredoc_opened_before_semicolon(ps, range) {
                corrector.replace(range, "\n");
            }
            return;
        }
        let mut node = None;
        if let Some(token) = &token_before {
            if matches!(token.ty, "tDOT2" | "tDOT3") {
                node = find_node(&range_nodes(ps), token);
            } else if token.ty == "tLABEL" {
                let pair = find_node(&value_omission_pair_nodes(ps), token);
                if let Some(hash) = pair.and_then(|p| p.parent()) {
                    if let Some(selector) = hash.parent().and_then(|p| p.loc().selector) {
                        corrector.remove(selector.end().join(hash.source_range().begin()));
                    }
                    node = Some(hash);
                }
            }
        }
        if let Some(node) = node {
            corrector.wrap(node.source_range(), "(", ")");
        }
        corrector.remove(range);
    });
}

fn heredoc_opened_before_semicolon(ps: &ProcessedSource<'_>, semicolon_range: Range) -> bool {
    let Some(ast) = ps.ast() else {
        return false;
    };
    ast.descendants()
        .filter(|d| d.is_any_str() && d.is_heredoc())
        .any(|heredoc| {
            heredoc.first_line() == ps.line_of(semicolon_range.begin_pos)
                && heredoc.source_range().end_pos <= semicolon_range.begin_pos
        })
}

fn find_node<'a>(nodes: &[Node<'a>], token: &Token) -> Option<Node<'a>> {
    nodes
        .iter()
        .find(|node| node.source_range().overlaps(token.range))
        .copied()
}

fn range_nodes<'a>(ps: &'a ProcessedSource<'_>) -> Vec<Node<'a>> {
    let Some(ast) = ps.ast() else {
        return Vec::new();
    };
    ast.each_node().filter(|n| n.is_range()).collect()
}

fn value_omission_pair_nodes<'a>(ps: &'a ProcessedSource<'_>) -> Vec<Node<'a>> {
    let Some(ast) = ps.ast() else {
        return Vec::new();
    };
    ast.descendants()
        .filter(|n| n.is(NodeType::Pair) && n.is_value_omission())
        .collect()
}
