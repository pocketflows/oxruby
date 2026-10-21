//! Port of syntax_tree's `lib/syntax_tree/formatter.rb` and of every node's
//! `format` method in `lib/syntax_tree/node.rb`, in the same order.

use std::collections::HashMap;

use crate::node::{Ast, CallOperator, Kind, NilOr, NodeId};
use crate::parser::is_stree_ignore;
use crate::prettier_print::{BufferKind, Doc, DocBuilder, DocId, Force, PrettierPrint, TextMode};

pub const COMMENT_PRIORITY: i64 = 1;
pub const HEREDOC_PRIORITY: i64 = 2;

/// `Formatter::Options`
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// `"` by default, `'` with the single_quotes plugin.
    pub quote: &'static str,
    pub trailing_comma: bool,
    pub disable_auto_ternary: bool,
    /// Only consulted by `HshPtn` (`< 2.7.3`).
    pub target_ruby_version: (u32, u32, u32),
}

impl Default for Options {
    fn default() -> Self {
        Options {
            quote: "\"",
            trailing_comma: false,
            disable_auto_ternary: false,
            target_ruby_version: (3, 3, 11),
        }
    }
}

/// Objects syntax_tree formats through `q.format` that aren't nodes. When
/// formatted with `stackable: true` they land on the stack, where every
/// `is_a?` check against them is false.
#[derive(Debug, Clone, Copy)]
pub enum Helper {
    AliasArgument(NodeId),
    CallOperator(CallOperator),
    AryPtnRest(NodeId),
    BlockOpen { text: BlockText, node: NodeId },
    HshPtnKeyword(NodeId, Option<NodeId>),
    HshPtnKeywordRest(NodeId),
    ParamsOptional(NodeId, NodeId),
    ParamsKeyword(NodeId, Option<NodeId>),
    ParamsKeywordRest(NilOr),
    UndefArgument(NodeId),
}

#[derive(Debug, Clone, Copy)]
pub enum BlockText {
    /// The opening token's own value.
    Value(NodeId),
    Static(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub enum Entry {
    Node(NodeId),
    Helper(Helper),
}

/// `HashKeyFormatter`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyFormatter {
    Labels,
    Rockets,
    Identity,
}

pub struct Formatter<'a> {
    pp: PrettierPrint,
    pub ast: &'a Ast,
    pub stack: Vec<Entry>,
    pub options: Options,
    key_formatters: HashMap<NodeId, KeyFormatter>,
}

impl DocBuilder for Formatter<'_> {
    fn pp(&mut self) -> &mut PrettierPrint {
        &mut self.pp
    }
}

/// `SyntaxTree.format_node`: formats the program and returns the source.
pub fn format(ast: &Ast, maxwidth: usize, base_indentation: usize, options: Options) -> String {
    let mut q = Formatter::new(ast, maxwidth, options);
    // Program#format is called directly, not through `q.format`, so the
    // program itself never lands on the stack.
    q.format_program(ast.root);
    q.pp.flush(base_indentation)
}

/// Formats a single node (used by the ERB plugin for Ruby statements).
pub fn format_node(ast: &Ast, node: NodeId, maxwidth: usize, options: Options) -> String {
    let mut q = Formatter::new(ast, maxwidth, options);
    q.format(node);
    q.pp.flush(0)
}

/// `String#each_line(chomp: true)`
pub fn each_line_chomp(value: &str) -> Vec<&str> {
    value
        .split_inclusive('\n')
        .map(|line| {
            let line = line.strip_suffix('\n').unwrap_or(line);
            line.strip_suffix('\r').unwrap_or(line)
        })
        .collect()
}

fn static_str(value: &str) -> &'static str {
    match value {
        "+" => "+",
        "-" => "-",
        "*" => "*",
        "/" => "/",
        "%" => "%",
        "**" => "**",
        "==" => "==",
        "===" => "===",
        "!=" => "!=",
        "=~" => "=~",
        "!~" => "!~",
        "<" => "<",
        "<=" => "<=",
        ">" => ">",
        ">=" => ">=",
        "<=>" => "<=>",
        "<<" => "<<",
        ">>" => ">>",
        "&" => "&",
        "|" => "|",
        "^" => "^",
        "&&" => "&&",
        "||" => "||",
        "and" => "and",
        "or" => "or",
        "=>" => "=>",
        "!" => "!",
        "~" => "~",
        _ => Box::leak(value.to_string().into_boxed_str()),
    }
}

impl<'a> Formatter<'a> {
    pub fn new(ast: &'a Ast, maxwidth: usize, options: Options) -> Self {
        Formatter {
            pp: PrettierPrint::new(maxwidth, TextMode::Raw, BufferKind::Array),
            ast,
            stack: Vec::new(),
            options,
            key_formatters: HashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Accessors
    // ------------------------------------------------------------------

    fn k(&self, id: NodeId) -> &'a Kind {
        self.ast.kind(id)
    }

    fn value(&self, id: NodeId) -> &'a str {
        self.ast.value(id)
    }

    fn comments(&self, id: NodeId) -> &'a [NodeId] {
        self.ast.comments(id)
    }

    fn text_value(&mut self, id: NodeId) {
        let value = self.value(id).to_string();
        self.text_owned(value);
    }

    fn entry_kind(&self, entry: Option<Entry>) -> Option<&'a Kind> {
        match entry {
            Some(Entry::Node(id)) => Some(self.k(id)),
            _ => None,
        }
    }

    /// `q.parent`
    fn parent(&self) -> Option<Entry> {
        if self.stack.len() >= 2 {
            Some(self.stack[self.stack.len() - 2])
        } else {
            None
        }
    }

    fn parent_kind(&self) -> Option<&'a Kind> {
        self.entry_kind(self.parent())
    }

    fn parent_node(&self) -> Option<NodeId> {
        match self.parent() {
            Some(Entry::Node(id)) => Some(id),
            _ => None,
        }
    }

    /// `q.grandparent`
    fn grandparent(&self) -> Option<Entry> {
        if self.stack.len() >= 3 {
            Some(self.stack[self.stack.len() - 3])
        } else {
            None
        }
    }

    /// `q.parents`: the stack without the current node, nearest first.
    fn parents(&self) -> Vec<Entry> {
        let mut parents: Vec<Entry> = self.stack[..self.stack.len().saturating_sub(1)].to_vec();
        parents.reverse();
        parents
    }

    fn is_comment_leading(&self, id: NodeId) -> bool {
        matches!(
            self.k(id),
            Kind::Comment { leading: true, .. } | Kind::EmbDoc { leading: true, .. }
        )
    }

    fn is_comment_inline(&self, id: NodeId) -> bool {
        matches!(self.k(id), Kind::Comment { inline: true, .. })
    }

    // ------------------------------------------------------------------
    // Formatter#format
    // ------------------------------------------------------------------

    pub fn format(&mut self, id: NodeId) -> Option<DocId> {
        self.format_entry(Entry::Node(id), true)
    }

    fn format_helper(&mut self, helper: Helper, stackable: bool) {
        self.format_entry(Entry::Helper(helper), stackable);
    }

    fn entry_comments(&self, entry: Entry) -> Vec<NodeId> {
        match entry {
            Entry::Node(id) => self.comments(id).to_vec(),
            Entry::Helper(helper) => match helper {
                Helper::AliasArgument(arg) | Helper::UndefArgument(arg) => {
                    let mut comments = self.comments(arg).to_vec();
                    if let Kind::SymbolLiteral { value } = self.k(arg) {
                        comments.extend_from_slice(self.comments(*value));
                    }
                    comments
                }
                Helper::CallOperator(op) => match op {
                    CallOperator::Node(id) => self.comments(id).to_vec(),
                    _ => vec![],
                },
                Helper::AryPtnRest(value) => self.comments(value).to_vec(),
                Helper::BlockOpen { node, .. } => self.comments(node).to_vec(),
                _ => vec![],
            },
        }
    }

    fn format_entry(&mut self, entry: Entry, stackable: bool) -> Option<DocId> {
        if stackable {
            self.stack.push(entry);
        }
        let comments = self.entry_comments(entry);
        let doc;
        if !comments.is_empty() {
            let mut trailing = Vec::new();
            let mut last_leading = None;
            for &comment in &comments {
                if self.is_comment_leading(comment) {
                    self.format_comment(comment);
                    self.breakable(" ", true, Force::Yes);
                    last_leading = Some(comment);
                } else {
                    trailing.push(comment);
                }
            }
            let ignore = last_leading.is_some_and(|c| match self.k(c) {
                Kind::Comment { value, .. } => is_stree_ignore(value),
                _ => false,
            });
            doc = if ignore && let Entry::Node(id) = entry {
                let loc = self.ast.loc(id);
                let range = &self.ast.source[loc.start_char..loc.end_char];
                let mut first = true;
                for line in each_line_chomp(range) {
                    if first {
                        first = false;
                    } else {
                        self.breakable_return();
                    }
                    self.text_owned(line.to_string());
                }
                if range.ends_with('\n') {
                    self.breakable_return();
                }
                None
            } else {
                self.format_body(entry)
            };
            for comment in trailing {
                let inline = self.is_comment_inline(comment);
                self.line_suffix(COMMENT_PRIORITY, |q| {
                    if inline {
                        q.text(" ");
                    } else {
                        q.breakable_space();
                    }
                    q.format_comment(comment);
                    q.break_parent();
                });
            }
        } else {
            doc = self.format_body(entry);
        }
        if stackable {
            self.stack.pop();
        }
        doc
    }

    fn format_body(&mut self, entry: Entry) -> Option<DocId> {
        match entry {
            Entry::Node(id) => self.format_node(id),
            Entry::Helper(helper) => {
                self.format_helper_body(helper);
                None
            }
        }
    }

    /// `Comment#format` / `EmbDoc#format`
    fn format_comment(&mut self, id: NodeId) {
        match self.k(id) {
            Kind::Comment { value, .. } => self.text_owned(value.clone()),
            Kind::EmbDoc { value, .. } => {
                let trim = match self.parent_kind() {
                    Some(Kind::DefNode { bodystmt, .. }) => {
                        !matches!(self.k(*bodystmt), Kind::BodyStmt { .. })
                    }
                    Some(Kind::Statements { .. }) => true,
                    _ => false,
                };
                if trim {
                    self.trim();
                } else {
                    self.breakable_return();
                }
                self.text_owned(value.clone());
            }
            _ => {}
        }
    }

    /// `seplist` with the default `comma_breakable` separator.
    fn seplist_nodes(&mut self, nodes: &[NodeId]) {
        for (index, &node) in nodes.iter().enumerate() {
            if index > 0 {
                self.comma_breakable();
            }
            self.format(node);
        }
    }

    fn format_program(&mut self, id: NodeId) {
        let Kind::Program { statements } = *self.k(id) else {
            return;
        };
        self.format(statements);
        let last_is_end = match self.k(statements) {
            Kind::Statements { body } => body
                .last()
                .is_some_and(|&last| matches!(self.k(last), Kind::EndContent { .. })),
            _ => false,
        };
        if !last_is_end {
            self.breakable_force();
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn format_helper_body(&mut self, helper: Helper) {
        match helper {
            Helper::AliasArgument(arg) | Helper::UndefArgument(arg) => {
                if let Kind::SymbolLiteral { value } = self.k(arg) {
                    self.format(*value);
                } else {
                    self.format(arg);
                }
            }
            Helper::CallOperator(op) => match op {
                CallOperator::Colon2 => self.text("."),
                CallOperator::Node(id) => {
                    // `operator.format(q)`: the node's own format, without
                    // comments or the stack.
                    if matches!(self.k(id), Kind::Op { value } if value == "::") {
                        self.text(".");
                    } else {
                        self.format_node(id);
                    }
                }
                CallOperator::Nil => {}
            },
            Helper::AryPtnRest(value) => {
                self.text("*");
                self.format(value);
            }
            Helper::BlockOpen { text, .. } => match text {
                BlockText::Value(node) => self.text_value(node),
                BlockText::Static(s) => self.text(s),
            },
            Helper::HshPtnKeyword(key, value) => {
                self.format_key_with(KeyFormatter::Labels, key);
                if let Some(value) = value {
                    self.text(" ");
                    self.format(value);
                }
            }
            Helper::HshPtnKeywordRest(rest) => {
                self.text("**");
                self.format(rest);
            }
            Helper::ParamsOptional(name, value) => {
                self.format(name);
                self.text(" = ");
                self.format(value);
            }
            Helper::ParamsKeyword(name, value) => {
                self.format(name);
                if let Some(value) = value {
                    self.text(" ");
                    self.format(value);
                }
            }
            Helper::ParamsKeywordRest(value) => match value {
                NilOr::NilSymbol => self.text("**nil"),
                NilOr::Node(id) => {
                    self.format(id);
                }
                NilOr::None => {}
            },
        }
    }

    // ------------------------------------------------------------------
    // Node#format
    // ------------------------------------------------------------------

    fn format_node(&mut self, id: NodeId) -> Option<DocId> {
        let kind = self.k(id);
        match kind {
            Kind::BEGINBlock { lbrace, statements } | Kind::ENDBlock { lbrace, statements } => {
                let keyword = if matches!(kind, Kind::BEGINBlock { .. }) {
                    "BEGIN "
                } else {
                    "END "
                };
                let (lbrace, statements) = (*lbrace, *statements);
                self.group(|q| {
                    q.text(keyword);
                    q.format(lbrace);
                    q.indent(|q| {
                        q.breakable_space();
                        q.format(statements);
                    });
                    q.breakable_space();
                    q.text("}");
                });
            }
            Kind::CHAR { value } => {
                let chars: Vec<char> = value.chars().collect();
                if chars.len() != 2 {
                    self.text_owned(value.clone());
                } else {
                    let quote = self.options.quote;
                    self.text(quote);
                    if chars[1].to_string() == quote {
                        self.text_owned(format!("\\{quote}"));
                    } else {
                        self.text_owned(chars[1].to_string());
                    }
                    self.text(quote);
                }
            }
            Kind::EndContent { value } => {
                self.text("__END__");
                self.breakable_force();
                let mut first = true;
                for line in each_line_chomp(value) {
                    if first {
                        first = false;
                    } else {
                        self.breakable_return();
                    }
                    self.text_owned(line.to_string());
                }
                if value.ends_with('\n') {
                    self.breakable_return();
                }
            }
            Kind::AliasNode { left, right } => {
                let (left, right) = (*left, *right);
                let keyword = "alias ";
                let left_has_comments = !self
                    .entry_comments(Entry::Helper(Helper::AliasArgument(left)))
                    .is_empty();
                self.group(|q| {
                    q.text(keyword);
                    q.format_helper(Helper::AliasArgument(left), false);
                    q.group(|q| {
                        q.nest(keyword.len() as i64, |q| {
                            if left_has_comments {
                                q.breakable_force();
                            } else {
                                q.breakable_space();
                            }
                            q.format_helper(Helper::AliasArgument(right), false);
                        });
                    });
                });
            }
            Kind::ARef { collection, index } | Kind::ARefField { collection, index } => {
                let (collection, index) = (*collection, *index);
                self.group(|q| {
                    q.format(collection);
                    q.text("[");
                    if let Some(index) = index {
                        q.indent(|q| {
                            q.breakable_empty();
                            q.format(index);
                        });
                        q.breakable_empty();
                    }
                    q.text("]");
                });
            }
            Kind::ArgParen { arguments } => {
                let Some(arguments) = *arguments else {
                    self.text("()");
                    return None;
                };
                let trailing =
                    self.options.trailing_comma && self.arg_paren_trailing_comma(arguments);
                self.text("(");
                self.group(|q| {
                    q.indent(|q| {
                        q.breakable_empty();
                        q.format(arguments);
                        if trailing {
                            q.if_break_only(|q| q.text(","));
                        }
                    });
                    q.breakable_empty();
                });
                self.text(")");
            }
            Kind::Args { parts } => {
                let parts = parts.clone();
                self.seplist_nodes(&parts);
            }
            Kind::ArgBlock { value } => {
                let value = *value;
                self.text("&");
                if let Some(value) = value {
                    self.format(value);
                }
            }
            Kind::ArgStar { value } => {
                let value = *value;
                self.text("*");
                if let Some(value) = value {
                    self.format(value);
                }
            }
            Kind::ArgsForward => self.text("..."),
            Kind::ArrayLiteral { lbracket, contents } => {
                self.format_array(*lbracket, *contents);
            }
            Kind::AryPtn {
                constant,
                requireds,
                rest,
                posts,
            } => {
                let (constant, requireds, rest, posts) =
                    (*constant, requireds.clone(), *rest, posts.clone());
                self.group(|q| {
                    if let Some(constant) = constant {
                        q.format(constant);
                    }
                    q.text("[");
                    q.indent(|q| {
                        q.breakable_empty();
                        let mut parts: Vec<Entry> =
                            requireds.iter().map(|&r| Entry::Node(r)).collect();
                        if let Some(rest) = rest {
                            parts.push(Entry::Helper(Helper::AryPtnRest(rest)));
                        }
                        parts.extend(posts.iter().map(|&p| Entry::Node(p)));
                        for (index, part) in parts.into_iter().enumerate() {
                            if index > 0 {
                                q.comma_breakable();
                            }
                            q.format_entry(part, true);
                        }
                    });
                    q.breakable_empty();
                    q.text("]");
                });
            }
            Kind::Assign { target, value } => {
                let (target, value) = (*target, *value);
                let skip = self.comments(target).is_empty()
                    && (matches!(self.k(target), Kind::ARefField { .. })
                        || self.skip_indent(value));
                self.group(|q| {
                    q.format(target);
                    q.text(" =");
                    if skip {
                        q.text(" ");
                        q.format(value);
                    } else {
                        q.indent(|q| {
                            q.breakable_space();
                            q.format(value);
                        });
                    }
                });
            }
            Kind::Assoc { key, value } => {
                let (key, value) = (*key, *value);
                if value.is_some_and(|v| matches!(self.k(v), Kind::HashLiteral { .. })) {
                    self.format_assoc_contents(key, value);
                } else {
                    self.group(|q| q.format_assoc_contents(key, value));
                }
            }
            Kind::AssocSplat { value } => {
                let value = *value;
                self.text("**");
                if let Some(value) = value {
                    self.format(value);
                }
            }
            Kind::Backref { value } | Kind::Backtick { value } => self.text_owned(value.clone()),
            Kind::BareAssocHash { assocs } => {
                let assocs = assocs.clone();
                self.seplist_nodes(&assocs);
            }
            Kind::Begin { bodystmt } => {
                let bodystmt = *bodystmt;
                self.text("begin");
                if !self.bodystmt_empty(bodystmt) {
                    let statements_empty = match self.k(bodystmt) {
                        Kind::BodyStmt { statements, .. } => self.statements_empty(*statements),
                        _ => false,
                    };
                    self.indent(|q| {
                        if !statements_empty {
                            q.breakable_force();
                        }
                        q.format(bodystmt);
                    });
                }
                self.breakable_force();
                self.text("end");
            }
            Kind::PinnedBegin { statement } => {
                let statement = *statement;
                self.group(|q| {
                    q.text("^(");
                    q.nest(1, |q| {
                        q.indent(|q| {
                            q.breakable_empty();
                            q.format(statement);
                        });
                        q.breakable_empty();
                        q.text(")");
                    });
                });
            }
            Kind::Binary {
                left,
                operator,
                right,
            } => {
                let (left, right) = (*left, *right);
                let operator = static_str(operator);
                self.format_binary(left, operator, right);
            }
            Kind::BlockVar { params, locals } => {
                let (params, locals) = (*params, locals.clone());
                self.text("|");
                self.group(|q| {
                    if let Some(doc) = q.format(params) {
                        q.pp.remove_breaks(doc, "; ");
                    }
                    if !locals.is_empty() {
                        q.text("; ");
                        for (index, &local) in locals.iter().enumerate() {
                            if index > 0 {
                                q.text(", ");
                            }
                            q.format(local);
                        }
                    }
                });
                self.text("|");
            }
            Kind::BlockArg { name } | Kind::KwRestParam { name } | Kind::RestParam { name } => {
                let prefix = match kind {
                    Kind::BlockArg { .. } => "&",
                    Kind::KwRestParam { .. } => "**",
                    _ => "*",
                };
                let name = *name;
                self.text(prefix);
                if let Some(name) = name {
                    self.format(name);
                }
            }
            Kind::BodyStmt {
                statements,
                rescue_clause,
                else_keyword,
                else_clause,
                ensure_clause,
            } => {
                let (statements, rescue_clause, else_keyword, else_clause, ensure_clause) = (
                    *statements,
                    *rescue_clause,
                    *else_keyword,
                    *else_clause,
                    *ensure_clause,
                );
                self.group(|q| {
                    if !q.statements_empty(statements) {
                        q.format(statements);
                    }
                    if let Some(rescue_clause) = rescue_clause {
                        q.nest(-2, |q| {
                            q.breakable_force();
                            q.format(rescue_clause);
                        });
                    }
                    if let Some(else_clause) = else_clause {
                        q.nest(-2, |q| {
                            q.breakable_force();
                            if let Some(else_keyword) = else_keyword {
                                q.format(else_keyword);
                            }
                        });
                        if !q.statements_empty(else_clause) {
                            q.breakable_force();
                            q.format(else_clause);
                        }
                    }
                    if let Some(ensure_clause) = ensure_clause {
                        q.nest(-2, |q| {
                            q.breakable_force();
                            q.format(ensure_clause);
                        });
                    }
                });
            }
            Kind::Break { arguments } => {
                let arguments = Some(*arguments);
                self.format_flow_control("break", arguments);
            }
            Kind::Next { arguments } => {
                let arguments = Some(*arguments);
                self.format_flow_control("next", arguments);
            }
            Kind::ReturnNode { arguments } => {
                let arguments = *arguments;
                self.format_flow_control("return", arguments);
            }
            Kind::CallNode { .. } => self.format_call(id),
            Kind::Case {
                keyword,
                value,
                consequent,
            } => {
                let (keyword, value, consequent) = (*keyword, *value, *consequent);
                self.group(|q| {
                    q.format(keyword);
                    if let Some(value) = value {
                        q.text(" ");
                        q.format(value);
                    }
                    q.breakable_force();
                    q.format(consequent);
                    q.breakable_force();
                    q.text("end");
                });
            }
            Kind::RAssign {
                value,
                operator,
                pattern,
            } => {
                let (value, operator, pattern) = (*value, *operator, *pattern);
                let direct = matches!(
                    self.k(pattern),
                    Kind::AryPtn { .. } | Kind::FndPtn { .. } | Kind::HshPtn { .. }
                );
                self.group(|q| {
                    q.format(value);
                    q.text(" ");
                    q.format(operator);
                    if direct {
                        q.text(" ");
                        q.format(pattern);
                    } else {
                        q.group(|q| {
                            q.indent(|q| {
                                q.breakable_space();
                                q.format(pattern);
                            });
                        });
                    }
                });
            }
            Kind::ClassDeclaration {
                constant,
                superclass,
                bodystmt,
            } => {
                let (constant, superclass, bodystmt) = (*constant, *superclass, *bodystmt);
                let empty = self.bodystmt_empty(bodystmt);
                self.group(|q| {
                    q.group(|q| {
                        q.text("class ");
                        q.format(constant);
                        if let Some(superclass) = superclass {
                            q.text(" < ");
                            q.format(superclass);
                        }
                    });
                    if !empty {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(bodystmt);
                        });
                    }
                    q.breakable_force();
                    q.text("end");
                });
            }
            Kind::Comma { .. } => {}
            Kind::Command {
                message,
                arguments,
                block,
            } => {
                let (message, arguments, block) = (*message, *arguments, *block);
                self.group(|q| {
                    q.format(message);
                    q.command_align(message, id, arguments);
                });
                if let Some(block) = block {
                    self.format(block);
                }
            }
            Kind::CommandCall { .. } => self.format_command_call(id),
            Kind::Comment { .. } | Kind::EmbDoc { .. } => self.format_comment(id),
            Kind::Const { value }
            | Kind::CVar { value }
            | Kind::ExcessedComma { value }
            | Kind::FloatLiteral { value }
            | Kind::GVar { value }
            | Kind::HeredocBeg { value }
            | Kind::HeredocEnd { value }
            | Kind::Ident { value }
            | Kind::Imaginary { value }
            | Kind::IVar { value }
            | Kind::Kw { value }
            | Kind::Label { value }
            | Kind::LBrace { value }
            | Kind::LBracket { value }
            | Kind::LParen { value }
            | Kind::Op { value }
            | Kind::Period { value }
            | Kind::RationalLiteral { value }
            | Kind::TStringContent { value } => self.text_owned(value.clone()),
            Kind::QSymbolsBeg { .. }
            | Kind::QWordsBeg { .. }
            | Kind::SymbolsBeg { .. }
            | Kind::WordsBeg { .. } => {}
            Kind::ConstPathField { parent, constant } | Kind::ConstPathRef { parent, constant } => {
                let (parent, constant) = (*parent, *constant);
                self.format(parent);
                self.text("::");
                self.format(constant);
            }
            Kind::ConstRef { constant } => {
                let constant = *constant;
                self.format(constant);
            }
            Kind::DefNode { .. } => self.format_def(id),
            Kind::Defined { value } => {
                let value = *value;
                self.text("defined?(");
                self.group(|q| {
                    q.indent(|q| {
                        q.breakable_empty();
                        q.format(value);
                    });
                    q.breakable_empty();
                });
                self.text(")");
            }
            Kind::BlockNode { .. } => self.format_block(id),
            Kind::RangeNode {
                left,
                operator,
                right,
            } => {
                let (left, operator, right) = (*left, *operator, *right);
                if let Some(left) = left {
                    self.format(left);
                }
                let op = self.value(operator).to_string();
                if matches!(
                    self.parent_kind(),
                    Some(Kind::IfNode { .. } | Kind::UnlessNode { .. })
                ) {
                    self.text_owned(format!(" {op} "));
                } else {
                    self.text_owned(op);
                }
                if let Some(right) = right {
                    self.format(right);
                }
            }
            Kind::DynaSymbol { .. } => self.format_dyna_symbol(id),
            Kind::Else {
                keyword,
                statements,
            } => {
                let (keyword, statements) = (*keyword, *statements);
                self.group(|q| {
                    q.format(keyword);
                    if !q.statements_empty(statements) {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(statements);
                        });
                    }
                });
            }
            Kind::Elsif {
                predicate,
                statements,
                consequent,
            } => {
                let (predicate, statements, consequent) = (*predicate, *statements, *consequent);
                self.group(|q| {
                    q.group(|q| {
                        q.text("elsif ");
                        q.nest("elsif".len() as i64 - 1, |q| {
                            q.format(predicate);
                        });
                    });
                    if !q.statements_empty(statements) {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(statements);
                        });
                    }
                    if let Some(consequent) = consequent {
                        q.group(|q| {
                            q.breakable_force();
                            q.format(consequent);
                        });
                    }
                });
            }
            Kind::Ensure {
                keyword,
                statements,
            } => {
                let (keyword, statements) = (*keyword, *statements);
                self.format(keyword);
                if !self.statements_empty(statements) {
                    self.indent(|q| {
                        q.breakable_force();
                        q.format(statements);
                    });
                }
            }
            Kind::Field {
                parent,
                operator,
                name,
            } => {
                let (parent, operator, name) = (*parent, *operator, *name);
                self.group(|q| {
                    q.format(parent);
                    q.format_helper(Helper::CallOperator(operator), false);
                    q.format(name);
                });
            }
            Kind::FndPtn {
                constant,
                left,
                values,
                right,
            } => {
                let (constant, left, values, right) = (*constant, *left, values.clone(), *right);
                if let Some(constant) = constant {
                    self.format(constant);
                }
                self.group(|q| {
                    q.text("[");
                    q.indent(|q| {
                        q.breakable_empty();
                        q.text("*");
                        q.format(left);
                        q.comma_breakable();
                        q.seplist_nodes(&values);
                        q.comma_breakable();
                        q.text("*");
                        q.format(right);
                    });
                    q.breakable_empty();
                    q.text("]");
                });
            }
            Kind::For {
                index,
                collection,
                statements,
            } => {
                let (index, collection, statements) = (*index, *collection, *statements);
                self.group(|q| {
                    q.text("for ");
                    q.group(|q| {
                        q.format(index);
                    });
                    q.text(" in ");
                    q.format(collection);
                    if !q.statements_empty(statements) {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(statements);
                        });
                    }
                    q.breakable_force();
                    q.text("end");
                });
            }
            Kind::HashLiteral { lbrace, assocs } => {
                let (lbrace, assocs) = (*lbrace, assocs.clone());
                if matches!(self.parent_kind(), Some(Kind::Assoc { .. })) {
                    self.format_hash_contents(id, lbrace, &assocs);
                } else {
                    self.group(|q| q.format_hash_contents(id, lbrace, &assocs));
                }
            }
            Kind::Heredoc {
                beginning,
                ending,
                parts,
                ..
            } => {
                let (beginning, ending, parts) = (*beginning, *ending, parts.clone());
                self.group(|q| {
                    q.format(beginning);
                    q.line_suffix(HEREDOC_PRIORITY, |q| {
                        q.group(|q| {
                            q.heredoc_separator();
                            for &part in &parts {
                                if let Kind::TStringContent { value } = q.k(part) {
                                    let mut first = true;
                                    for line in each_line_chomp(value) {
                                        if first {
                                            first = false;
                                        } else {
                                            q.heredoc_separator();
                                        }
                                        q.text_owned(line.to_string());
                                    }
                                    if value.ends_with('\n') {
                                        q.heredoc_separator();
                                    }
                                } else {
                                    q.format(part);
                                }
                            }
                            if let Some(ending) = ending {
                                q.format(ending);
                            }
                        });
                    });
                });
            }
            Kind::HshPtn { .. } => self.format_hshptn(id),
            Kind::IfNode { .. } | Kind::UnlessNode { .. } => self.format_conditional(id),
            Kind::IfOp { .. } => self.format_ifop(id),
            Kind::In {
                pattern,
                statements,
                consequent,
            } => {
                let (pattern, statements, consequent) = (*pattern, *statements, *consequent);
                let keyword = "in ";
                let then = matches!(self.k(pattern), Kind::RangeNode { right: None, .. });
                self.group(|q| {
                    q.text(keyword);
                    q.nest(keyword.len() as i64, |q| {
                        q.format(pattern);
                    });
                    if then {
                        q.text(" then");
                    }
                    if !q.statements_empty(statements) {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(statements);
                        });
                    }
                    if let Some(consequent) = consequent {
                        q.breakable_force();
                        q.format(consequent);
                    }
                });
            }
            Kind::Int { value } => {
                let starts_zero = value.starts_with('0') || value.starts_with("+0");
                if !starts_zero && value.chars().count() >= 5 && !value.contains('_') {
                    let padded: Vec<char> = format!("  {value}").chars().collect();
                    let index = (value.chars().count() + 2) % 3;
                    let rest = &padded[index..];
                    let groups: Vec<String> = rest
                        .chunks(3)
                        .filter(|chunk| chunk.len() == 3)
                        .map(|chunk| chunk.iter().collect())
                        .collect();
                    self.text_owned(groups.join("_").trim().to_string());
                } else {
                    self.text_owned(value.clone());
                }
            }
            Kind::Lambda { .. } => self.format_lambda(id),
            Kind::LambdaVar { params, locals } => {
                let (params, locals) = (*params, locals.clone());
                self.format(params);
                if !locals.is_empty() {
                    self.text("; ");
                    for (index, &local) in locals.iter().enumerate() {
                        if index > 0 {
                            self.text(", ");
                        }
                        self.format(local);
                    }
                }
            }
            Kind::MAssign { target, value } => {
                let (target, value) = (*target, *value);
                self.group(|q| {
                    q.group(|q| {
                        q.format(target);
                    });
                    q.text(" =");
                    q.indent(|q| {
                        q.breakable_space();
                        q.format(value);
                    });
                });
            }
            Kind::MethodAddBlock { call, .. } => {
                let call = *call;
                if self.chained(Some(Entry::Node(call))) && !self.chained(self.parent()) {
                    self.group(|q| {
                        q.if_break(
                            |q| q.format_call_chain(id),
                            |q| q.format_method_add_block_contents(id),
                        );
                    });
                } else {
                    self.format_method_add_block_contents(id);
                }
            }
            Kind::MLHS { parts, comma } => {
                let (parts, comma) = (parts.clone(), *comma);
                self.seplist_nodes(&parts);
                if comma {
                    self.text(",");
                }
            }
            Kind::MLHSParen { contents, comma } => {
                let (contents, comma) = (*contents, *comma);
                if matches!(
                    self.parent_kind(),
                    Some(Kind::MAssign { .. } | Kind::MLHSParen { .. })
                ) {
                    self.format(contents);
                    if comma {
                        self.text(",");
                    }
                } else {
                    self.text("(");
                    self.group(|q| {
                        q.indent(|q| {
                            q.breakable_empty();
                            q.format(contents);
                        });
                        if comma {
                            q.text(",");
                        }
                        q.breakable_empty();
                    });
                    self.text(")");
                }
            }
            Kind::ModuleDeclaration { constant, bodystmt } => {
                let (constant, bodystmt) = (*constant, *bodystmt);
                let empty = self.bodystmt_empty(bodystmt);
                self.group(|q| {
                    q.group(|q| {
                        q.text("module ");
                        q.format(constant);
                    });
                    if !empty {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(bodystmt);
                        });
                    }
                    q.breakable_force();
                    q.text("end");
                });
            }
            Kind::MRHS { parts } => {
                let parts = parts.clone();
                self.seplist_nodes(&parts);
            }
            Kind::OpAssign {
                target,
                operator,
                value,
            } => {
                let (target, operator, value) = (*target, *operator, *value);
                let skip = self.comments(target).is_empty()
                    && (matches!(self.k(target), Kind::ARefField { .. })
                        || self.skip_indent(value));
                self.group(|q| {
                    q.format(target);
                    q.text(" ");
                    q.format(operator);
                    if skip {
                        q.text(" ");
                        q.format(value);
                    } else {
                        q.indent(|q| {
                            q.breakable_space();
                            q.format(value);
                        });
                    }
                });
            }
            Kind::Params { .. } => return Some(self.format_params(id)),
            Kind::Paren { lparen, contents } => {
                let (lparen, contents) = (*lparen, *contents);
                let show = contents.is_some_and(|c| {
                    !matches!(self.k(c), Kind::Params { .. }) || !self.params_empty(c)
                });
                self.group(|q| {
                    q.format(lparen);
                    if show {
                        q.indent(|q| {
                            q.breakable_empty();
                            q.format(contents.unwrap());
                        });
                    }
                    q.breakable_empty();
                    q.text(")");
                });
            }
            Kind::Program { .. } => self.format_program(id),
            Kind::QSymbols {
                beginning,
                elements,
            }
            | Kind::QWords {
                beginning,
                elements,
            }
            | Kind::Symbols {
                beginning,
                elements,
            }
            | Kind::Words {
                beginning,
                elements,
            } => {
                let default_opening = match kind {
                    Kind::QSymbols { .. } => "%i[",
                    Kind::QWords { .. } => "%w[",
                    Kind::Symbols { .. } => "%I[",
                    _ => "%W[",
                };
                let (beginning, elements) = (*beginning, elements.clone());
                let brackets = elements.iter().any(|&e| self.element_matches_brackets(e));
                let (opening, closing): (String, String) = if brackets {
                    let opening = self.value(beginning).to_string();
                    let delimiter = opening.chars().nth(2).unwrap_or('[');
                    let closing = quotes_matching(delimiter).to_string();
                    (opening, closing)
                } else {
                    (String::new(), String::new())
                };
                if brackets {
                    self.text_owned(opening);
                } else {
                    self.text(default_opening);
                }
                self.group(|q| {
                    q.indent(|q| {
                        q.breakable_empty();
                        for (index, &element) in elements.iter().enumerate() {
                            if index > 0 {
                                q.breakable_space();
                            }
                            q.format(element);
                        }
                    });
                    q.breakable_empty();
                });
                if brackets {
                    self.text_owned(closing);
                } else {
                    self.text("]");
                }
            }
            Kind::Redo => self.text("redo"),
            Kind::RegexpLiteral { .. } => self.format_regexp(id),
            Kind::RescueEx {
                exceptions,
                variable,
            } => {
                let (exceptions, variable) = (*exceptions, *variable);
                self.group(|q| {
                    if let Some(exceptions) = exceptions {
                        q.text(" ");
                        q.format(exceptions);
                    }
                    if let Some(variable) = variable {
                        q.text(" => ");
                        q.format(variable);
                    }
                });
            }
            Kind::Rescue {
                keyword,
                exception,
                statements,
                consequent,
            } => {
                let (keyword, exception, statements, consequent) =
                    (*keyword, *exception, *statements, *consequent);
                let width = self.value(keyword).chars().count() as i64 + 1;
                self.group(|q| {
                    q.format(keyword);
                    if let Some(exception) = exception {
                        q.nest(width, |q| {
                            q.format(exception);
                        });
                    } else {
                        q.text(" StandardError");
                    }
                    if !q.statements_empty(statements) {
                        q.indent(|q| {
                            q.breakable_force();
                            q.format(statements);
                        });
                    }
                    if let Some(consequent) = consequent {
                        q.breakable_force();
                        q.format(consequent);
                    }
                });
            }
            Kind::RescueMod { statement, value } => {
                let (statement, value) = (*statement, *value);
                self.text("begin");
                self.group(|q| {
                    q.indent(|q| {
                        q.breakable_force();
                        q.format(statement);
                    });
                    q.breakable_force();
                    q.text("rescue StandardError");
                    q.indent(|q| {
                        q.breakable_force();
                        q.format(value);
                    });
                    q.breakable_force();
                });
                self.text("end");
            }
            Kind::Retry => self.text("retry"),
            Kind::SClass { target, bodystmt } => {
                let (target, bodystmt) = (*target, *bodystmt);
                self.text("class << ");
                self.group(|q| {
                    q.format(target);
                    q.indent(|q| {
                        q.breakable_force();
                        q.format(bodystmt);
                    });
                    q.breakable_force();
                });
                self.text("end");
            }
            Kind::Statements { .. } => self.format_statements(id),
            Kind::StringConcat { left, right } => {
                let (left, right) = (*left, *right);
                self.group(|q| {
                    q.format(left);
                    q.text(" \\");
                    q.indent(|q| {
                        q.breakable_force();
                        q.format(right);
                    });
                });
            }
            Kind::StringDVar { variable } => {
                let variable = *variable;
                self.text("#{");
                self.format(variable);
                self.text("}");
            }
            Kind::StringEmbExpr { statements } => {
                let statements = *statements;
                let loc = self.ast.loc(id);
                if loc.start_line == loc.end_line {
                    let doc = self.group(|q| {
                        q.text("#{");
                        q.format(statements);
                        q.text("}");
                    });
                    self.pp.remove_breaks(doc, "; ");
                } else {
                    self.group(|q| {
                        q.text("#{");
                        q.indent(|q| {
                            q.breakable_empty();
                            q.format(statements);
                        });
                        q.breakable_empty();
                        q.text("}");
                    });
                }
            }
            Kind::StringLiteral { .. } => self.format_string_literal(id),
            Kind::Super { arguments } => {
                let arguments = *arguments;
                self.group(|q| {
                    q.text("super");
                    if matches!(q.k(arguments), Kind::ArgParen { .. }) {
                        q.format(arguments);
                    } else {
                        q.text(" ");
                        q.nest("super ".len() as i64, |q| {
                            q.format(arguments);
                        });
                    }
                });
            }
            Kind::SymbolLiteral { value } => {
                let value = *value;
                self.text(":");
                if !self.comments(value).is_empty() {
                    self.text("\\");
                }
                self.format(value);
            }
            Kind::TopConstField { constant } | Kind::TopConstRef { constant } => {
                let constant = *constant;
                self.text("::");
                self.format(constant);
            }
            Kind::Not { .. } => self.format_not(id),
            Kind::Unary {
                operator,
                statement,
            } => {
                let (operator, statement) = (operator.clone(), *statement);
                self.text_owned(operator);
                self.format(statement);
            }
            Kind::Undef { symbols } => {
                let symbols = symbols.clone();
                let keyword = "undef ";
                self.group(|q| {
                    q.text(keyword);
                    q.nest(keyword.len() as i64, |q| {
                        for (index, &symbol) in symbols.iter().enumerate() {
                            if index > 0 {
                                q.comma_breakable();
                            }
                            q.format_helper(Helper::UndefArgument(symbol), true);
                        }
                    });
                });
            }
            Kind::UntilNode { .. } | Kind::WhileNode { .. } => self.format_loop(id),
            Kind::VarField { value } => match *value {
                NilOr::NilSymbol => self.text("nil"),
                NilOr::Node(value) => {
                    self.format(value);
                }
                NilOr::None => {}
            },
            Kind::VarRef { value } | Kind::VCall { value } => {
                let value = *value;
                self.format(value);
            }
            Kind::PinnedVarRef { value } => {
                let value = *value;
                self.group(|q| {
                    q.text("^");
                    q.format(value);
                });
            }
            Kind::VoidStmt => {}
            Kind::When { .. } => self.format_when(id),
            Kind::Word { parts } => {
                let parts = parts.clone();
                for part in parts {
                    self.format(part);
                }
            }
            Kind::XStringLiteral { parts } => {
                let parts = parts.clone();
                self.text("`");
                for part in parts {
                    self.format(part);
                }
                self.text("`");
            }
            Kind::YieldNode { arguments } => {
                let arguments = *arguments;
                let Some(arguments) = arguments else {
                    self.text("yield");
                    return None;
                };
                self.group(|q| {
                    q.text("yield");
                    if matches!(q.k(arguments), Kind::Paren { .. }) {
                        q.format(arguments);
                    } else {
                        q.if_break(|q| q.text("("), |q| q.text(" "));
                        q.indent(|q| {
                            q.breakable_empty();
                            q.format(arguments);
                        });
                        q.breakable_empty();
                        q.if_break_only(|q| q.text(")"));
                    }
                });
            }
            Kind::ZSuper => self.text("super"),
        }
        None
    }

    fn heredoc_separator(&mut self) {
        // Heredoc::SEPARATOR: a forced newline without a break parent.
        self.pp.push(Doc::Breakable {
            separator: " ",
            width: 1,
            force: true,
            indent: false,
        });
    }

    // ------------------------------------------------------------------
    // Predicates
    // ------------------------------------------------------------------

    /// `Statements#empty?`
    pub fn statements_empty(&self, id: NodeId) -> bool {
        match self.k(id) {
            Kind::Statements { body } => body
                .iter()
                .all(|&s| matches!(self.k(s), Kind::VoidStmt) && self.comments(s).is_empty()),
            _ => false,
        }
    }

    /// `BodyStmt#empty?`
    fn bodystmt_empty(&self, id: NodeId) -> bool {
        match self.k(id) {
            Kind::BodyStmt {
                statements,
                rescue_clause,
                else_clause,
                ensure_clause,
                ..
            } => {
                self.statements_empty(*statements)
                    && rescue_clause.is_none()
                    && else_clause.is_none()
                    && ensure_clause.is_none()
            }
            // Endless defs and brace blocks hold Statements.
            Kind::Statements { .. } => self.statements_empty(id),
            _ => false,
        }
    }

    fn params_empty(&self, id: NodeId) -> bool {
        match self.k(id) {
            Kind::Params {
                requireds,
                optionals,
                rest,
                posts,
                keywords,
                keyword_rest,
                block,
            } => {
                requireds.is_empty()
                    && optionals.is_empty()
                    && rest.is_none()
                    && posts.is_empty()
                    && keywords.is_empty()
                    && *keyword_rest == NilOr::None
                    && block.is_none()
            }
            _ => false,
        }
    }

    /// `AssignFormatting.skip_indent?`
    fn skip_indent(&self, value: NodeId) -> bool {
        match self.k(value) {
            Kind::ArrayLiteral { .. }
            | Kind::HashLiteral { .. }
            | Kind::Heredoc { .. }
            | Kind::Lambda { .. }
            | Kind::QSymbols { .. }
            | Kind::QWords { .. }
            | Kind::Symbols { .. }
            | Kind::Words { .. } => true,
            Kind::CallNode { receiver, .. } => receiver.is_some_and(|r| self.skip_indent(r)),
            Kind::DynaSymbol { quote, .. } => quote.as_deref().is_some_and(|q| q.starts_with("%s")),
            _ => false,
        }
    }

    /// `ContainsAssignment.call`
    fn contains_assignment(&self, node: NodeId) -> bool {
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(node);
        while let Some(node) = queue.pop_front() {
            match self.k(node) {
                Kind::Assign { .. } | Kind::MAssign { .. } | Kind::OpAssign { .. } => return true,
                _ => {
                    for child in self.ast.child_nodes(node).into_iter().flatten() {
                        queue.push_back(child);
                    }
                }
            }
        }
        false
    }

    fn element_matches_brackets(&self, element: NodeId) -> bool {
        let matches = |v: &str| v.contains('[') || v.contains(']');
        match self.k(element) {
            Kind::TStringContent { value } => matches(value),
            Kind::Word { parts } => parts.iter().any(|&p| match self.k(p) {
                Kind::TStringContent { value } => matches(value),
                _ => false,
            }),
            _ => false,
        }
    }
}

/// `Quotes.matching`
pub fn quotes_matching(quote: char) -> char {
    match quote {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        other => other,
    }
}

/// `Quotes.normalize`: escape unescaped quotes that match the enclosing one.
pub fn quotes_normalize(content: &str, enclosing: &str) -> String {
    if enclosing != "\"" && enclosing != "'" {
        return content.to_string();
    }
    let enclosing = enclosing.chars().next().unwrap();
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(escaped) => {
                    out.push('\\');
                    out.push(escaped);
                }
                None => out.push('\\'),
            }
        } else if c == '"' || c == '\'' {
            if c == enclosing {
                out.push('\\');
            }
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

// The bigger formatters live in their own impl blocks.
include!("format_nodes.rs");
