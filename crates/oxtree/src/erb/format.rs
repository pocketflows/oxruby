//! Port of w_syntax_tree-erb 0.12.0's `lib/syntax_tree/erb/format.rb` (and
//! of `ErbEmbeddedFormatter::Format#visit_html` from the app, which swaps in
//! pre-formatted `<script>`/`<style>` bodies).

use crate::erb::parser::{
    Attr, BlockKind, Document, Elem, ErbClose, ErbContent, ErbNode, HtmlAttribute, HtmlString,
    NewLine, OpeningTag, StringPart, T, Token, YIELD_PLACEHOLDER, statement_children,
};
use crate::prettier_print::{BufferKind, DocBuilder, Force, PrettierPrint, TextMode};

/// `SyntaxTree::ERB::MAX_WIDTH`: Ruby inside ERB tags is always formatted at
/// this width, whatever the document's width.
pub const MAX_WIDTH: usize = 80;

pub struct Format {
    pp: PrettierPrint,
    /// Pre-formatted lines for `__STREE_EMBED_<n>__` placeholders.
    embeds: Option<Vec<Vec<String>>>,
    /// Options for the Ruby inside tags: upstream builds a fresh
    /// `Formatter::Options`, which reads the loaded plugins' constants.
    options: crate::format::Options,
}

impl DocBuilder for Format {
    fn pp(&mut self) -> &mut PrettierPrint {
        &mut self.pp
    }
}

/// `PrettierPrint.format(+"", maxwidth) { |q| Format.new(q).visit(document) }`
pub fn format_document(
    document: &Document,
    maxwidth: usize,
    embeds: Option<Vec<Vec<String>>>,
) -> String {
    format_document_with_options(
        document,
        maxwidth,
        embeds,
        crate::format::Options::default(),
    )
}

/// `format_document`, with the plugin options the Ruby inside tags uses.
pub fn format_document_with_options(
    document: &Document,
    maxwidth: usize,
    embeds: Option<Vec<Vec<String>>>,
    options: crate::format::Options,
) -> String {
    let mut q = Format {
        pp: PrettierPrint::new(maxwidth, TextMode::TextNode, BufferKind::String),
        embeds,
        options,
    };
    q.visit_document(document);
    q.pp.flush(0)
}

/// Formats a single element (the app's `format_erb_tag`).
pub fn format_elements(document: &Document, maxwidth: usize) -> String {
    format_document(document, maxwidth, None)
}

impl Format {
    fn visit(&mut self, node: &Elem) {
        match node {
            Elem::Html { .. } => self.visit_html(node),
            Elem::Erb(erb) => self.visit_erb(erb),
            Elem::ErbEnd(erb) => self.visit_erb_end(erb),
            Elem::Block { .. } => self.visit_block(node),
            Elem::CharData { value, .. } => {
                if !value.value.trim().is_empty() {
                    self.text_owned(value.value.clone());
                }
            }
            Elem::NewLine(n) => self.visit_new_line(n),
            Elem::HtmlComment { token, .. } => self.visit_token(token),
            Elem::ErbComment { token, .. } => {
                let lines = ruby_split_lines(&token.value);
                for (index, line) in lines.iter().enumerate() {
                    if index > 0 {
                        self.breakable_space();
                    }
                    self.text_owned(line.trim_start().to_string());
                }
            }
            Elem::Doctype {
                opening,
                name,
                closing,
                ..
            } => {
                self.group(|q| {
                    q.visit_token(opening);
                    q.text(" ");
                    q.visit_token(name);
                    q.visit_token(closing);
                });
            }
            Elem::ErbYield { .. } => self.text("yield"),
        }
    }

    fn visit_token(&mut self, node: &Token) {
        if matches!(node.ty, T::Text | T::Whitespace) {
            self.text_owned(node.value.clone());
        } else {
            self.text_owned(node.value.trim().to_string());
        }
    }

    fn visit_document(&mut self, node: &Document) {
        let mut children: Vec<&Elem> = node.elements.iter().collect();
        children.sort_by_key(|c| c.location().start_char);
        let children: Vec<Elem> = children.into_iter().cloned().collect();
        self.handle_child_nodes(&children);
        self.breakable(" ", true, Force::Yes);
    }

    fn visit_block(&mut self, node: &Elem) {
        let Elem::Block {
            opening,
            elements,
            closing,
            ..
        } = node
        else {
            return;
        };
        self.visit_erb(opening);
        let breakable = true;
        if !elements.is_empty() {
            self.indent(|q| {
                if breakable {
                    q.breakable_space();
                }
                q.handle_child_nodes(elements);
            });
        }
        if breakable {
            self.breakable_empty();
        }
        self.visit(closing);
    }

    fn visit_html_groupable(&mut self, node: &Elem, group: bool) {
        let Elem::Html {
            opening,
            elements,
            closing,
            ..
        } = node
        else {
            return;
        };
        if elements.is_empty() {
            self.visit_opening_tag(opening);
            if let Some(closing) = closing {
                self.visit_closing_tag(closing);
            }
        } else {
            self.visit_opening_tag(opening);
            let with_break = self.breakable_inside(node);
            self.indent(|q| {
                if with_break {
                    if group {
                        q.breakable_empty();
                    } else {
                        q.breakable_space();
                    }
                }
                q.handle_child_nodes(elements);
            });
            if with_break {
                if group {
                    self.breakable_empty();
                } else {
                    self.breakable_space();
                }
            }
            if let Some(closing) = closing {
                self.visit_closing_tag(closing);
            }
        }
    }

    fn visit_html(&mut self, node: &Elem) {
        if self.embeds.is_some() && self.visit_embedded_html(node) {
            return;
        }
        let Elem::Html { elements, .. } = node else {
            return;
        };
        let grouped = elements.is_empty()
            || elements.iter().any(|e| matches!(e, Elem::CharData { .. }))
            || (elements.len() == 1 && matches!(elements[0], Elem::Erb(_)));
        if grouped {
            self.group(|q| q.visit_html_groupable(node, true));
        } else {
            self.visit_html_groupable(node, false);
        }
    }

    /// `ErbEmbeddedFormatter::Format#visit_html`
    fn visit_embedded_html(&mut self, node: &Elem) -> bool {
        let Elem::Html {
            opening,
            elements,
            closing,
            ..
        } = node
        else {
            return false;
        };
        if elements.len() != 1 {
            return false;
        }
        let Elem::CharData { value, .. } = &elements[0] else {
            return false;
        };
        let placeholder = value.value.trim();
        let Some(index) = placeholder
            .strip_prefix("__STREE_EMBED_")
            .and_then(|rest| rest.strip_suffix("__"))
            .filter(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
            .and_then(|digits| digits.parse::<usize>().ok())
        else {
            return false;
        };
        let Some(lines) = self.embeds.as_ref().and_then(|e| e.get(index)).cloned() else {
            return false;
        };
        self.visit_opening_tag(opening);
        self.indent(|q| {
            for line in &lines {
                q.breakable(" ", true, Force::Yes);
                q.text_owned(line.clone());
            }
        });
        self.breakable(" ", true, Force::Yes);
        if let Some(closing) = closing {
            self.visit_closing_tag(closing);
        }
        true
    }

    fn visit_erb(&mut self, node: &ErbNode) {
        self.visit_token(&node.opening_tag);
        if let Some(keyword) = &node.keyword {
            self.text(" ");
            self.visit_token(keyword);
        }
        if node.content.blank() {
            self.text(" ");
        } else {
            self.visit_erb_content(&node.content);
        }
        self.visit_erb_close(&node.closing_tag);
    }

    fn visit_erb_close(&mut self, node: &ErbClose) {
        if node.do_close {
            let value = &node.closing.value;
            let closing = if value.ends_with("-%>") { "-%>" } else { "%>" };
            self.text_owned(value.replace(closing, "").trim_end().to_string());
            self.text(" ");
            self.text(closing);
        } else {
            self.visit_token(&node.closing);
        }
    }

    fn visit_erb_end(&mut self, node: &ErbNode) {
        self.visit_token(&node.opening_tag);
        self.text(" ");
        if let Some(keyword) = &node.keyword {
            self.visit_token(keyword);
        }
        self.text(" ");
        self.visit_erb_close(&node.closing_tag);
    }

    fn visit_erb_content(&mut self, node: &ErbContent) {
        let Some(ast) = &node.value else {
            return;
        };
        let nodes = statement_children(ast);
        if nodes.len() == 1 {
            self.text(" ");
            self.format_statement(ast, nodes[0]);
            self.text(" ");
        } else if nodes.len() > 1 {
            self.indent(|q| {
                q.breakable_empty();
                for (index, &child) in nodes.iter().enumerate() {
                    if index > 0 {
                        q.breakable_empty();
                    }
                    q.format_statement(ast, child);
                }
            });
            self.breakable_space();
        }
    }

    fn format_statement(&mut self, ast: &crate::node::Ast, statement: crate::node::NodeId) {
        let formatted = crate::format::format_node(ast, statement, MAX_WIDTH, self.options)
            .replace(YIELD_PLACEHOLDER, "yield");
        let rows = ruby_split_lines(&formatted);
        self.output_rows(&rows);
    }

    fn output_rows(&mut self, rows: &[String]) {
        if rows.len() > 1 {
            for (index, row) in rows.iter().enumerate() {
                if index > 0 {
                    self.breakable_empty();
                }
                self.text_owned(row.clone());
            }
        } else if rows.len() == 1 {
            self.text_owned(rows[0].clone());
        }
    }

    fn visit_opening_tag(&mut self, node: &OpeningTag) {
        self.group(|q| {
            q.visit_token(&node.opening);
            q.visit_token(&node.name);
            if !node.attributes.is_empty() {
                q.indent(|q| {
                    q.breakable_space();
                    for (index, attr) in node.attributes.iter().enumerate() {
                        if index > 0 {
                            q.breakable_space();
                        }
                        match attr {
                            Attr::Erb(e) => q.visit(e),
                            Attr::Html(a) => q.visit_attribute(a),
                        }
                    }
                });
                if node.closing.value == "/>" {
                    q.breakable_space();
                } else {
                    q.breakable_empty();
                }
            } else if node.closing.value == "/>" {
                q.text(" ");
            }
            if crate::erb::parser::is_void_element(&node.name.value) && node.closing.value == ">" {
                q.text(" /");
            }
            q.visit_token(&node.closing);
        });
    }

    fn visit_closing_tag(&mut self, node: &crate::erb::parser::ClosingTag) {
        self.group(|q| {
            q.visit_token(&node.opening);
            q.visit_token(&node.name);
            q.visit_token(&node.closing);
        });
    }

    fn visit_attribute(&mut self, node: &HtmlAttribute) {
        self.group(|q| {
            q.visit_token(&node.key);
            if let Some(equals) = &node.equals {
                q.visit_token(equals);
            }
            if let Some(value) = &node.value {
                q.visit_html_string(value);
            }
        });
    }

    fn visit_html_string(&mut self, node: &HtmlString) {
        self.group(|q| {
            q.text("\"");
            for part in &node.contents {
                match part {
                    StringPart::Token(t) => q.visit_token(t),
                    StringPart::Erb(e) => q.visit(e),
                }
            }
            q.text("\"");
        });
    }

    fn visit_new_line(&mut self, node: &NewLine) {
        self.breakable(" ", true, Force::SkipBreakParent);
        if node.count > 1 {
            self.breakable(" ", true, Force::SkipBreakParent);
        }
    }

    // ------------------------------------------------------------------
    // private
    // ------------------------------------------------------------------

    fn breakable_inside(&self, node: &Elem) -> bool {
        match node {
            Elem::Html {
                opening, elements, ..
            } => {
                !matches!(elements.first(), Some(Elem::CharData { .. }))
                    || opening.new_line.as_ref().map_or(0, |n| n.count) > 0
            }
            Elem::Block { .. } => true,
            _ => false,
        }
    }

    fn node_new_line_count(node: &Elem) -> usize {
        node.new_line().map_or(0, |n| n.count)
    }

    fn breakable_between(&mut self, node: &Elem, next_node: Option<&Elem>) {
        let new_lines = Self::node_new_line_count(node);
        if new_lines == 1 {
            self.breakable_space();
        } else if new_lines > 1 {
            self.breakable_space();
            self.breakable(" ", true, Force::SkipBreakParent);
        } else if let Some(next) = next_node
            && !matches!(node, Elem::CharData { .. })
            && !matches!(next, Elem::CharData { .. })
        {
            self.breakable_space();
        }
    }

    fn breakable_between_group(&mut self, node: &Elem, next_node: Option<&Elem>) {
        let new_lines = Self::node_new_line_count(node);
        if new_lines == 1 {
            self.breakable(" ", true, Force::Yes);
        } else if new_lines > 1 {
            self.breakable(" ", true, Force::Yes);
            self.breakable(" ", true, Force::Yes);
        } else if let Some(next) = next_node
            && !matches!(node, Elem::CharData { .. })
            && !matches!(next, Elem::CharData { .. })
        {
            self.breakable_empty();
        }
    }

    fn handle_child_nodes(&mut self, child_nodes: &[Elem]) {
        let mut group: Vec<Elem> = Vec::new();
        if child_nodes.len() == 1 {
            let node = child_nodes[0].without_new_line();
            self.visit(&node);
            return;
        }
        for (index, child) in child_nodes.iter().enumerate() {
            let is_last = index == child_nodes.len() - 1;
            let node = if is_last {
                child.without_new_line()
            } else {
                child.clone()
            };
            if node_should_group(&node) {
                group.push(node);
                continue;
            }
            self.handle_group(&group, true);
            group.clear();
            self.visit(&node);
            let next_node = child_nodes.get(index + 1);
            self.breakable_between(&node, next_node);
        }
        self.handle_group(&group, false);
    }

    fn handle_group(&mut self, nodes: &[Elem], break_after: bool) {
        if nodes.len() == 1 {
            self.handle_group_nodes(nodes);
        } else if nodes.len() > 1 {
            self.group(|q| q.handle_group_nodes(nodes));
        } else {
            return;
        }
        if break_after {
            self.breakable_between_group(nodes.last().unwrap(), None);
        }
    }

    fn handle_group_nodes(&mut self, nodes: &[Elem]) {
        for (index, node) in nodes.iter().enumerate() {
            self.visit(node);
            let Some(next) = nodes.get(index + 1) else {
                continue;
            };
            self.breakable_between_group(node, Some(next));
        }
    }
}

fn node_should_group(node: &Elem) -> bool {
    matches!(node, Elem::CharData { .. } | Elem::Erb(_))
}

/// `String#split("\n")`: trailing empty strings are dropped.
pub fn ruby_split_lines(s: &str) -> Vec<String> {
    let mut parts: Vec<String> = s.split('\n').map(|p| p.to_string()).collect();
    while parts.last().is_some_and(|p| p.is_empty()) {
        parts.pop();
    }
    parts
}

#[allow(dead_code)]
fn block_kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::ErbBlock => "ErbBlock",
        BlockKind::ErbIf => "ErbIf",
        BlockKind::ErbUnless => "ErbUnless",
        BlockKind::ErbElsif => "ErbElsif",
        BlockKind::ErbElse => "ErbElse",
        BlockKind::ErbCase => "ErbCase",
        BlockKind::ErbCaseWhen => "ErbCaseWhen",
    }
}
