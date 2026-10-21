//! Builds `processed_source.tokens` in safe Rust.
//!
//! RuboCop's tokens come from `Prism::Translation::Parser::Lexer`, which
//! converts the stream `Prism.lex` produces. The crates.io `ruby-prism` has no
//! lexer API, so [`prism_tokens`] rebuilds that stream instead:
//!
//! - every token whose location Prism's tree records (keywords, operators,
//!   names, literals, delimiters) comes from a walk over the tree, typed by
//!   the node it belongs to;
//! - comments come from the parse result;
//! - the gaps between those tokens hold only whitespace, `,`, `;`, `\`
//!   continuations and newlines in code, and string content inside literals,
//!   so a small scanner classifies them. Newlines are `NEWLINE` or
//!   `IGNORED_NEWLINE` depending on the lexer state the previous token leaves,
//!   which is tracked the way Prism's lexer (`parser_lex` in `prism.c`) does;
//! - heredoc bodies are emitted right after their `HEREDOC_START`, as Prism
//!   lexes them.
//!
//! The result feeds the existing port of the translation lexer (`lexer.rs`).

use std::collections::{HashMap, HashSet};

use ruby_prism::{Location, Node, Visit};

use crate::ast::lexer::{Lexer, RawToken, Token};
use crate::processed_source::ProcessedSource;

// Prism's lexer states (`pm_lex_state_t`).
const BEG: u32 = 1 << 0;
const END: u32 = 1 << 1;
const ENDARG: u32 = 1 << 2;
const ENDFN: u32 = 1 << 3;
const ARG: u32 = 1 << 4;
const CMDARG: u32 = 1 << 5;
const MID: u32 = 1 << 6;
const FNAME: u32 = 1 << 7;
const DOT: u32 = 1 << 8;
const CLASS: u32 = 1 << 9;
const LABEL: u32 = 1 << 10;
const LABELED: u32 = 1 << 11;
const FITEM: u32 = 1 << 12;

/// `processed_source.tokens`, in the `parser` gem's format.
pub fn tokens(ps: &ProcessedSource<'_>) -> Vec<Token> {
    let raw = prism_tokens(ps);
    Lexer::new(ps.buffer_source(), &raw).tokens()
}

/// The token stream `Prism.lex` would produce (without `EOF`).
pub fn prism_tokens(ps: &ProcessedSource<'_>) -> Vec<RawToken> {
    build(ps).0
}

/// Like [`prism_tokens`], plus the byte offsets of gap text the scanner could
/// not classify (each one means a token the tree walk missed).
pub fn prism_tokens_checked(ps: &ProcessedSource<'_>) -> (Vec<RawToken>, Vec<usize>) {
    build(ps)
}

fn build(ps: &ProcessedSource<'_>) -> (Vec<RawToken>, Vec<usize>) {
    let src = ps.buffer_source().as_bytes();
    let mut collector = Collector {
        src,
        toks: Vec::new(),
        heredocs: Vec::new(),
        binary_prefix: HashSet::new(),
        parents: Vec::new(),
        marker: std::marker::PhantomData,
    };
    collector.visit(&ps.prism_ast());
    let Collector {
        mut toks,
        heredocs,
        binary_prefix,
        ..
    } = collector;
    for tok in &mut toks {
        if binary_prefix.contains(&tok.start) {
            tok.ty = match tok.ty {
                "USTAR" => "STAR",
                "USTAR_STAR" => "STAR_STAR",
                "UAMPERSAND" => "AMPERSAND",
                other => other,
            };
        }
    }

    let result = ps.parse_result();
    for comment in result.comments() {
        let loc = comment.location();
        let (start, end) = (loc.start() as usize, loc.end() as usize);
        if src[start..].starts_with(b"=begin") {
            let mut line_start = start;
            let mut first = true;
            while line_start < end {
                let line_end = src[line_start..end]
                    .iter()
                    .position(|&b| b == b'\n')
                    .map_or(end, |i| line_start + i + 1);
                let ty = if first {
                    "EMBDOC_BEGIN"
                } else if line_end >= end {
                    "EMBDOC_END"
                } else {
                    "EMBDOC_LINE"
                };
                toks.push(Tok::new(ty, line_start, line_end, St::Keep));
                first = false;
                line_start = line_end;
            }
        } else {
            let end = if src.get(end) == Some(&b'\n') {
                end + 1
            } else {
                end
            };
            toks.push(Tok::new("COMMENT", start, end, St::Comment));
        }
    }
    let data_start = result.data_loc().map(|loc| loc.start() as usize);
    if let Some(start) = data_start {
        toks.push(Tok::new("__END__", start, src.len(), St::Keep));
    }

    toks.sort_by_key(|t| (t.start, t.end));
    toks.dedup_by(|b, a| a.start == b.start && a.end == b.end);

    let mut emitter = Emitter::new(src, toks, heredocs, ps.ruby_version() >= 4.0);
    emitter.run(data_start.unwrap_or(src.len()));
    (emitter.out, emitter.unknown)
}

/// How a token changes the lexer state.
#[derive(Clone, Copy, Debug)]
enum St {
    /// Leaves it alone (string delimiters, content, embedded documents).
    Keep,
    Set(u32),
    /// A `(`: `PARENTHESIS_LEFT_PARENTHESES` after a space in an argument
    /// position, `PARENTHESIS_LEFT` otherwise.
    Paren,
    /// The `)` of a `ParenthesesNode` (`ENDARG` after `foo (1)`).
    ParenClose(usize),
    /// A comment, which also consumes the newline after it.
    Comment,
}

/// What a gap between tokens holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Code,
    /// String, symbol, regexp and xstring content: one `STRING_CONTENT`.
    Str,
    /// `%w`/`%i` content: `STRING_CONTENT` separated by `WORDS_SEP`.
    List,
    /// A `<<~` body: one `STRING_CONTENT` per line.
    Squiggly,
    /// Any other heredoc body.
    Heredoc,
}

#[derive(Clone, Copy, Debug)]
enum Ctx {
    None,
    Push(Mode),
    Pop,
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    ty: &'static str,
    start: usize,
    end: usize,
    st: St,
    ctx: Ctx,
    /// For `HEREDOC_START`: the heredoc's index.
    heredoc: Option<usize>,
}

impl Tok {
    fn new(ty: &'static str, start: usize, end: usize, st: St) -> Self {
        Self {
            ty,
            start,
            end,
            st,
            ctx: Ctx::None,
            heredoc: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Heredoc {
    open_end: usize,
    close_start: usize,
    close_end: usize,
    squiggly: bool,
}

fn span(loc: &Location<'_>) -> (usize, usize) {
    (loc.start() as usize, loc.end() as usize)
}

struct Collector<'s, 'pr> {
    src: &'s [u8],
    toks: Vec<Tok>,
    heredocs: Vec<Heredoc>,
    /// Where a paren-less `def`'s parameters start.
    binary_prefix: HashSet<usize>,
    /// For each enclosing node, whether it is an `ArrayNode`.
    parents: Vec<bool>,
    marker: std::marker::PhantomData<&'pr ()>,
}

impl<'pr> Collector<'_, 'pr> {
    fn push(&mut self, ty: &'static str, loc: &Location<'_>, st: St) {
        let (start, end) = span(loc);
        self.toks.push(Tok::new(ty, start, end, st));
    }

    fn push_opt(&mut self, ty: &'static str, loc: Option<Location<'_>>, st: St) {
        if let Some(loc) = loc {
            self.push(ty, &loc, st);
        }
    }

    fn push_ctx(&mut self, ty: &'static str, loc: &Location<'_>, st: St, ctx: Ctx) {
        let (start, end) = span(loc);
        let mut tok = Tok::new(ty, start, end, st);
        tok.ctx = ctx;
        self.toks.push(tok);
    }

    fn text(&self, loc: &Location<'_>) -> &[u8] {
        let (start, end) = span(loc);
        &self.src[start..end]
    }

    fn keyword(&mut self, loc: Option<Location<'_>>, state: u32) {
        if let Some(loc) = loc {
            let ty = keyword_type(self.text(&loc)).unwrap_or("IDENTIFIER");
            self.push(ty, &loc, St::Set(state));
        }
    }

    /// An `end`. (An `ElseNode` before `ensure` ends at the `ensure`, which
    /// is the `EnsureNode`'s token.)
    fn end_keyword(&mut self, loc: Option<Location<'_>>) {
        if let Some(loc) = loc
            && self.text(&loc) == b"end"
        {
            self.push("KEYWORD_END", &loc, St::Set(END));
        }
    }

    /// A method or symbol name, typed as Prism's lexer types it.
    fn name(&mut self, loc: &Location<'_>, keywords: bool, state: u32) {
        let ty = name_type(self.text(loc), keywords);
        self.push(ty, loc, St::Set(state));
    }

    fn paren_open(&mut self, loc: Option<Location<'_>>) {
        self.push_opt("PARENTHESIS_LEFT", loc, St::Paren);
    }

    fn paren_close(&mut self, loc: Option<Location<'_>>) {
        self.push_opt("PARENTHESIS_RIGHT", loc, St::Set(ENDFN));
    }

    fn operator(&mut self, loc: Option<Location<'_>>, state: u32) {
        if let Some(loc) = loc {
            let ty = operator_type(self.text(&loc)).unwrap_or("EQUAL");
            self.push(ty, &loc, St::Set(state));
        }
    }

    /// The opening and closing of a string-like literal.
    fn literal(&mut self, open: Option<Location<'_>>, close: Option<Location<'_>>, mode: Mode) {
        let Some(open) = open else {
            return;
        };
        let text = self.text(&open);
        if text.starts_with(b"<<") {
            if let Some(close) = close {
                self.heredoc(&open, &close);
            }
            return;
        }
        let ty = match text {
            b"`" => "BACKTICK",
            _ if text.starts_with(b"%x") => "PERCENT_LOWER_X",
            _ if text.starts_with(b"%w") => "PERCENT_LOWER_W",
            _ if text.starts_with(b"%W") => "PERCENT_UPPER_W",
            _ if text.starts_with(b"%i") => "PERCENT_LOWER_I",
            _ if text.starts_with(b"%I") => "PERCENT_UPPER_I",
            _ if text.starts_with(b":") || text.starts_with(b"%s") => "SYMBOL_BEGIN",
            _ if mode == Mode::Str && (text.starts_with(b"/") || text.starts_with(b"%r")) => {
                "REGEXP_BEGIN"
            }
            _ => "STRING_BEGIN",
        };
        let st = if ty == "SYMBOL_BEGIN" {
            St::Set(if text.starts_with(b"%s") {
                FNAME | FITEM
            } else {
                FNAME
            })
        } else {
            St::Keep
        };
        self.push_ctx(ty, &open, st, Ctx::Push(mode));
        if let Some(close) = close {
            let text = self.text(&close);
            let (ty, st) = if ty == "REGEXP_BEGIN" {
                ("REGEXP_END", END)
            } else if text.len() > 1 && text.ends_with(b":") {
                ("LABEL_END", ARG | LABELED)
            } else {
                ("STRING_END", END)
            };
            self.push_ctx(ty, &close, St::Set(st), Ctx::Pop);
        }
    }

    fn regexp(&mut self, open: Location<'_>, close: Location<'_>) {
        self.push_ctx("REGEXP_BEGIN", &open, St::Keep, Ctx::Push(Mode::Str));
        self.push_ctx("REGEXP_END", &close, St::Set(END), Ctx::Pop);
    }

    fn heredoc(&mut self, open: &Location<'_>, close: &Location<'_>) {
        let (open_start, open_end) = span(open);
        let (close_start, close_end) = span(close);
        let index = self.heredocs.len();
        self.heredocs.push(Heredoc {
            open_end,
            close_start,
            close_end,
            squiggly: self.src[open_start..].starts_with(b"<<~"),
        });
        let mut tok = Tok::new("HEREDOC_START", open_start, open_end, St::Keep);
        tok.heredoc = Some(index);
        self.toks.push(tok);
        self.toks.push(Tok::new(
            "HEREDOC_END",
            close_start,
            close_end,
            St::Set(END),
        ));
    }

    fn number(&mut self, loc: &Location<'_>) {
        let (start, end) = span(loc);
        let text = &self.src[start..end];
        let mut start = start;
        if text.starts_with(b"-") {
            self.toks
                .push(Tok::new("UMINUS_NUM", start, start + 1, St::Set(BEG)));
            start += 1;
        }
        let ty = number_type(&self.src[start..end]);
        self.toks.push(Tok::new(ty, start, end, St::Set(END)));
    }

    /// Whether the statements of a modifier form come before its keyword.
    fn is_modifier(statements: Option<Node<'_>>, keyword: &Location<'_>) -> bool {
        statements.is_some_and(|s| s.location().start() < keyword.start())
    }

    fn in_array(&self) -> bool {
        self.parents.last().copied().unwrap_or(false)
    }

    #[allow(clippy::too_many_lines)]
    fn enter(&mut self, node: &Node<'pr>) {
        match node {
            Node::AliasGlobalVariableNode { .. } => {
                let n = node.as_alias_global_variable_node().unwrap();
                self.keyword(Some(n.keyword_loc()), FNAME | FITEM);
            }
            Node::AliasMethodNode { .. } => {
                let n = node.as_alias_method_node().unwrap();
                self.keyword(Some(n.keyword_loc()), FNAME | FITEM);
            }
            Node::AlternationPatternNode { .. } => {
                let n = node.as_alternation_pattern_node().unwrap();
                self.push("PIPE", &n.operator_loc(), St::Set(BEG | LABEL));
            }
            Node::AndNode { .. } => {
                let n = node.as_and_node().unwrap();
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::OrNode { .. } => {
                let n = node.as_or_node().unwrap();
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::ArrayNode { .. } => {
                let n = node.as_array_node().unwrap();
                match n.opening_loc() {
                    Some(open) if self.text(&open) == b"[" => {
                        self.push("BRACKET_LEFT_ARRAY", &open, St::Set(BEG | LABEL));
                        self.push_opt("BRACKET_RIGHT", n.closing_loc(), St::Set(END));
                    }
                    open => self.literal(open, n.closing_loc(), Mode::List),
                }
            }
            Node::ArrayPatternNode { .. } => {
                let n = node.as_array_pattern_node().unwrap();
                self.pattern_delimiters(n.constant().is_some(), n.opening_loc(), n.closing_loc());
            }
            Node::FindPatternNode { .. } => {
                let n = node.as_find_pattern_node().unwrap();
                self.pattern_delimiters(n.constant().is_some(), n.opening_loc(), n.closing_loc());
            }
            Node::HashPatternNode { .. } => {
                let n = node.as_hash_pattern_node().unwrap();
                self.pattern_delimiters(n.constant().is_some(), n.opening_loc(), n.closing_loc());
            }
            Node::AssocNode { .. } => {
                let n = node.as_assoc_node().unwrap();
                self.push_opt("EQUAL_GREATER", n.operator_loc(), St::Set(BEG));
            }
            Node::AssocSplatNode { .. } => {
                let n = node.as_assoc_splat_node().unwrap();
                self.push("USTAR_STAR", &n.operator_loc(), St::Set(BEG));
            }
            Node::BackReferenceReadNode { .. } => {
                self.push("BACK_REFERENCE", &node.location(), St::Set(END));
            }
            Node::BeginNode { .. } => {
                let n = node.as_begin_node().unwrap();
                self.keyword(n.begin_keyword_loc(), BEG);
                self.end_keyword(n.end_keyword_loc());
            }
            Node::BlockArgumentNode { .. } => {
                let n = node.as_block_argument_node().unwrap();
                self.push("UAMPERSAND", &n.operator_loc(), St::Set(BEG));
            }
            Node::BlockLocalVariableNode { .. } => {
                self.push("IDENTIFIER", &node.location(), St::Set(ARG));
            }
            Node::BlockNode { .. } => {
                let n = node.as_block_node().unwrap();
                self.block_delimiters(n.opening_loc(), n.closing_loc(), "BRACE_LEFT");
            }
            Node::LambdaNode { .. } => {
                let n = node.as_lambda_node().unwrap();
                self.push("MINUS_GREATER", &n.operator_loc(), St::Set(ENDFN));
                self.block_delimiters(n.opening_loc(), n.closing_loc(), "LAMBDA_BEGIN");
            }
            Node::BlockParameterNode { .. } => {
                let n = node.as_block_parameter_node().unwrap();
                self.push("UAMPERSAND", &n.operator_loc(), St::Set(BEG));
                self.push_opt("IDENTIFIER", n.name_loc(), St::Set(ARG));
            }
            Node::NoBlockParameterNode { .. } => {
                let n = node.as_no_block_parameter_node().unwrap();
                self.push("UAMPERSAND", &n.operator_loc(), St::Set(BEG));
                self.push("KEYWORD_NIL", &n.keyword_loc(), St::Set(END));
            }
            Node::BlockParametersNode { .. } => {
                let n = node.as_block_parameters_node().unwrap();
                for loc in [n.opening_loc(), n.closing_loc()].into_iter().flatten() {
                    match self.text(&loc) {
                        b"(" => self.paren_open(Some(loc)),
                        b")" => self.paren_close(Some(loc)),
                        _ => self.push("PIPE", &loc, St::Set(BEG | LABEL)),
                    }
                }
            }
            Node::BreakNode { .. } => {
                let n = node.as_break_node().unwrap();
                self.keyword(Some(n.keyword_loc()), MID);
            }
            Node::NextNode { .. } => {
                let n = node.as_next_node().unwrap();
                self.keyword(Some(n.keyword_loc()), MID);
            }
            Node::ReturnNode { .. } => {
                let n = node.as_return_node().unwrap();
                self.keyword(Some(n.keyword_loc()), MID);
            }
            Node::CallNode { .. } => self.call_node(node),
            Node::CallAndWriteNode { .. } => {
                let n = node.as_call_and_write_node().unwrap();
                self.call_operator(n.call_operator_loc());
                if let Some(msg) = n.message_loc() {
                    self.name(&msg, false, ARG);
                }
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::CallOrWriteNode { .. } => {
                let n = node.as_call_or_write_node().unwrap();
                self.call_operator(n.call_operator_loc());
                if let Some(msg) = n.message_loc() {
                    self.name(&msg, false, ARG);
                }
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::CallOperatorWriteNode { .. } => {
                let n = node.as_call_operator_write_node().unwrap();
                self.call_operator(n.call_operator_loc());
                if let Some(msg) = n.message_loc() {
                    self.name(&msg, false, ARG);
                }
                self.operator(Some(n.binary_operator_loc()), BEG);
            }
            Node::CallTargetNode { .. } => {
                let n = node.as_call_target_node().unwrap();
                self.call_operator(Some(n.call_operator_loc()));
                self.name(&n.message_loc(), false, ARG);
            }
            Node::IndexAndWriteNode { .. } => {
                let n = node.as_index_and_write_node().unwrap();
                self.call_operator(n.call_operator_loc());
                self.index_brackets(&n.opening_loc(), &n.closing_loc());
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::IndexOrWriteNode { .. } => {
                let n = node.as_index_or_write_node().unwrap();
                self.call_operator(n.call_operator_loc());
                self.index_brackets(&n.opening_loc(), &n.closing_loc());
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::IndexOperatorWriteNode { .. } => {
                let n = node.as_index_operator_write_node().unwrap();
                self.call_operator(n.call_operator_loc());
                self.index_brackets(&n.opening_loc(), &n.closing_loc());
                self.operator(Some(n.binary_operator_loc()), BEG);
            }
            Node::IndexTargetNode { .. } => {
                let n = node.as_index_target_node().unwrap();
                self.index_brackets(&n.opening_loc(), &n.closing_loc());
            }
            Node::CapturePatternNode { .. } => {
                let n = node.as_capture_pattern_node().unwrap();
                self.push("EQUAL_GREATER", &n.operator_loc(), St::Set(BEG));
            }
            Node::CaseNode { .. } => {
                let n = node.as_case_node().unwrap();
                self.keyword(Some(n.case_keyword_loc()), BEG);
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::CaseMatchNode { .. } => {
                let n = node.as_case_match_node().unwrap();
                self.keyword(Some(n.case_keyword_loc()), BEG);
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::WhenNode { .. } => {
                let n = node.as_when_node().unwrap();
                self.keyword(Some(n.when_keyword_loc()), BEG);
                self.keyword(n.then_keyword_loc(), BEG);
            }
            Node::InNode { .. } => {
                let n = node.as_in_node().unwrap();
                self.keyword(Some(n.in_keyword_loc()), BEG | LABEL);
                self.keyword(n.then_keyword_loc(), BEG);
            }
            Node::ClassNode { .. } => {
                let n = node.as_class_node().unwrap();
                self.keyword(Some(n.class_keyword_loc()), CLASS);
                self.push_opt("LESS", n.inheritance_operator_loc(), St::Set(BEG));
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::SingletonClassNode { .. } => {
                let n = node.as_singleton_class_node().unwrap();
                self.keyword(Some(n.class_keyword_loc()), CLASS);
                self.push("LESS_LESS", &n.operator_loc(), St::Set(BEG));
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::ModuleNode { .. } => {
                let n = node.as_module_node().unwrap();
                self.keyword(Some(n.module_keyword_loc()), BEG);
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::ClassVariableReadNode { .. } | Node::ClassVariableTargetNode { .. } => {
                self.push("CLASS_VARIABLE", &node.location(), St::Set(END));
            }
            Node::InstanceVariableReadNode { .. } | Node::InstanceVariableTargetNode { .. } => {
                self.push("INSTANCE_VARIABLE", &node.location(), St::Set(END));
            }
            Node::GlobalVariableReadNode { .. } | Node::GlobalVariableTargetNode { .. } => {
                let ty = name_type(self.text(&node.location()), false);
                self.push(ty, &node.location(), St::Set(END));
            }
            Node::NumberedReferenceReadNode { .. } => {
                self.push("NUMBERED_REFERENCE", &node.location(), St::Set(END));
            }
            Node::LocalVariableReadNode { .. }
            | Node::LocalVariableTargetNode { .. }
            | Node::ItLocalVariableReadNode { .. } => {
                self.push("IDENTIFIER", &node.location(), St::Set(END | LABEL));
            }
            Node::RequiredParameterNode { .. } => {
                self.push("IDENTIFIER", &node.location(), St::Set(ARG));
            }
            Node::ConstantReadNode { .. } | Node::ConstantTargetNode { .. } => {
                self.push("CONSTANT", &node.location(), St::Set(CMDARG));
            }
            Node::ClassVariableWriteNode { .. }
            | Node::ClassVariableAndWriteNode { .. }
            | Node::ClassVariableOrWriteNode { .. }
            | Node::ClassVariableOperatorWriteNode { .. } => {
                self.variable_write(node, "CLASS_VARIABLE", END);
            }
            Node::InstanceVariableWriteNode { .. }
            | Node::InstanceVariableAndWriteNode { .. }
            | Node::InstanceVariableOrWriteNode { .. }
            | Node::InstanceVariableOperatorWriteNode { .. } => {
                self.variable_write(node, "INSTANCE_VARIABLE", END);
            }
            Node::GlobalVariableWriteNode { .. }
            | Node::GlobalVariableAndWriteNode { .. }
            | Node::GlobalVariableOrWriteNode { .. }
            | Node::GlobalVariableOperatorWriteNode { .. } => {
                self.variable_write(node, "GLOBAL_VARIABLE", END);
            }
            Node::LocalVariableWriteNode { .. }
            | Node::LocalVariableAndWriteNode { .. }
            | Node::LocalVariableOrWriteNode { .. }
            | Node::LocalVariableOperatorWriteNode { .. } => {
                self.variable_write(node, "IDENTIFIER", END | LABEL);
            }
            Node::ConstantWriteNode { .. }
            | Node::ConstantAndWriteNode { .. }
            | Node::ConstantOrWriteNode { .. }
            | Node::ConstantOperatorWriteNode { .. } => {
                self.variable_write(node, "CONSTANT", CMDARG);
            }
            Node::ConstantPathAndWriteNode { .. } => {
                let n = node.as_constant_path_and_write_node().unwrap();
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::ConstantPathOrWriteNode { .. } => {
                let n = node.as_constant_path_or_write_node().unwrap();
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::ConstantPathOperatorWriteNode { .. } => {
                let n = node.as_constant_path_operator_write_node().unwrap();
                self.operator(Some(n.binary_operator_loc()), BEG);
            }
            Node::ConstantPathWriteNode { .. } => {
                let n = node.as_constant_path_write_node().unwrap();
                self.operator(Some(n.operator_loc()), BEG);
            }
            Node::ConstantPathNode { .. } => {
                let n = node.as_constant_path_node().unwrap();
                self.constant_path(n.parent().is_some(), &n.delimiter_loc(), &n.name_loc());
            }
            Node::ConstantPathTargetNode { .. } => {
                let n = node.as_constant_path_target_node().unwrap();
                self.constant_path(n.parent().is_some(), &n.delimiter_loc(), &n.name_loc());
            }
            Node::DefNode { .. } => {
                let n = node.as_def_node().unwrap();
                self.keyword(Some(n.def_keyword_loc()), FNAME);
                // Without parentheses the first parameter is lexed right
                // after the name (`ENDFN`), where `*`, `**` and `&` are
                // binary operators: `def each &block`.
                if n.lparen_loc().is_none()
                    && let Some(params) = n.parameters()
                {
                    self.binary_prefix
                        .insert(params.location().start() as usize);
                }
                if let Some(op) = n.operator_loc() {
                    let ty = if self.text(&op) == b"::" {
                        "COLON_COLON"
                    } else {
                        "DOT"
                    };
                    self.push(ty, &op, St::Set(FNAME));
                }
                self.name(&n.name_loc(), true, ENDFN);
                self.paren_open(n.lparen_loc());
                self.push_opt("PARENTHESIS_RIGHT", n.rparen_loc(), St::Set(BEG));
                self.push_opt("EQUAL", n.equal_loc(), St::Set(BEG));
                self.end_keyword(n.end_keyword_loc());
            }
            Node::DefinedNode { .. } => {
                let n = node.as_defined_node().unwrap();
                self.push("KEYWORD_DEFINED", &n.keyword_loc(), St::Set(ARG));
                self.paren_open(n.lparen_loc());
                self.paren_close(n.rparen_loc());
            }
            Node::ElseNode { .. } => {
                let n = node.as_else_node().unwrap();
                let loc = n.else_keyword_loc();
                if self.text(&loc) == b":" {
                    self.push("COLON", &loc, St::Set(BEG));
                } else {
                    self.keyword(Some(loc), BEG);
                }
                self.end_keyword(n.end_keyword_loc());
            }
            Node::EnsureNode { .. } => {
                let n = node.as_ensure_node().unwrap();
                self.keyword(Some(n.ensure_keyword_loc()), BEG);
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::EmbeddedStatementsNode { .. } => {
                let n = node.as_embedded_statements_node().unwrap();
                self.push_ctx(
                    "EMBEXPR_BEGIN",
                    &n.opening_loc(),
                    St::Set(BEG),
                    Ctx::Push(Mode::Code),
                );
                self.push_ctx("EMBEXPR_END", &n.closing_loc(), St::Keep, Ctx::Pop);
            }
            Node::EmbeddedVariableNode { .. } => {
                let n = node.as_embedded_variable_node().unwrap();
                self.push("EMBVAR", &n.operator_loc(), St::Set(BEG));
            }
            Node::FalseNode { .. }
            | Node::TrueNode { .. }
            | Node::NilNode { .. }
            | Node::SelfNode { .. }
            | Node::SourceFileNode { .. }
            | Node::SourceLineNode { .. }
            | Node::SourceEncodingNode { .. }
            | Node::RedoNode { .. }
            | Node::RetryNode { .. } => {
                self.keyword(Some(node.location()), END);
            }
            Node::FlipFlopNode { .. } => {
                let n = node.as_flip_flop_node().unwrap();
                self.range_operator(n.left().is_some(), &n.operator_loc());
            }
            Node::RangeNode { .. } => {
                let n = node.as_range_node().unwrap();
                self.range_operator(n.left().is_some(), &n.operator_loc());
            }
            Node::FloatNode { .. } | Node::IntegerNode { .. } | Node::RationalNode { .. } => {
                self.number(&node.location());
            }
            Node::ImaginaryNode { .. } => self.number(&node.location()),
            Node::ForNode { .. } => {
                let n = node.as_for_node().unwrap();
                self.keyword(Some(n.for_keyword_loc()), BEG);
                self.keyword(Some(n.in_keyword_loc()), BEG);
                self.push_opt("KEYWORD_DO_LOOP", n.do_keyword_loc(), St::Set(BEG));
                self.end_keyword(Some(n.end_keyword_loc()));
            }
            Node::ForwardingArgumentsNode { .. } => {
                self.push("UDOT_DOT_DOT", &node.location(), St::Set(BEG));
            }
            Node::ForwardingParameterNode { .. } => {
                self.push("UDOT_DOT_DOT", &node.location(), St::Set(ENDARG));
            }
            Node::ForwardingSuperNode { .. } => {
                let n = node.as_forwarding_super_node().unwrap();
                self.keyword(Some(n.keyword_loc()), ARG);
            }
            Node::SuperNode { .. } => {
                let n = node.as_super_node().unwrap();
                self.keyword(Some(n.keyword_loc()), ARG);
                self.paren_open(n.lparen_loc());
                self.paren_close(n.rparen_loc());
            }
            Node::YieldNode { .. } => {
                let n = node.as_yield_node().unwrap();
                self.keyword(Some(n.keyword_loc()), ARG);
                self.paren_open(n.lparen_loc());
                self.paren_close(n.rparen_loc());
            }
            Node::HashNode { .. } => {
                let n = node.as_hash_node().unwrap();
                self.push("BRACE_LEFT", &n.opening_loc(), St::Set(BEG | LABEL));
                self.push("BRACE_RIGHT", &n.closing_loc(), St::Set(END));
            }
            Node::IfNode { .. } => {
                let n = node.as_if_node().unwrap();
                match n.if_keyword_loc() {
                    Some(kw) if self.text(&kw) == b"elsif" => {
                        self.push("KEYWORD_ELSIF", &kw, St::Set(BEG));
                    }
                    Some(kw) => {
                        let statements = n.statements().map(|s| s.as_node());
                        if Self::is_modifier(statements, &kw) {
                            self.push("KEYWORD_IF_MODIFIER", &kw, St::Set(BEG | LABEL));
                        } else {
                            self.push("KEYWORD_IF", &kw, St::Set(BEG));
                        }
                    }
                    None => {}
                }
                if let Some(then) = n.then_keyword_loc() {
                    if self.text(&then) == b"?" {
                        self.push("QUESTION_MARK", &then, St::Set(BEG));
                    } else {
                        self.keyword(Some(then), BEG);
                    }
                }
                self.end_keyword(n.end_keyword_loc());
            }
            Node::UnlessNode { .. } => {
                let n = node.as_unless_node().unwrap();
                let kw = n.unless_keyword_loc();
                let statements = n.statements().map(|s| s.as_node());
                if Self::is_modifier(statements, &kw) {
                    self.push("KEYWORD_UNLESS_MODIFIER", &kw, St::Set(BEG | LABEL));
                } else {
                    self.push("KEYWORD_UNLESS", &kw, St::Set(BEG));
                }
                self.keyword(n.then_keyword_loc(), BEG);
                self.end_keyword(n.end_keyword_loc());
            }
            Node::WhileNode { .. } => {
                let n = node.as_while_node().unwrap();
                let kw = n.while_keyword_loc();
                let statements = n.statements().map(|s| s.as_node());
                if Self::is_modifier(statements, &kw) {
                    self.push("KEYWORD_WHILE_MODIFIER", &kw, St::Set(BEG | LABEL));
                } else {
                    self.push("KEYWORD_WHILE", &kw, St::Set(BEG));
                }
                self.push_opt("KEYWORD_DO_LOOP", n.do_keyword_loc(), St::Set(BEG));
                self.end_keyword(n.end_keyword_loc());
            }
            Node::UntilNode { .. } => {
                let n = node.as_until_node().unwrap();
                let kw = n.until_keyword_loc();
                let statements = n.statements().map(|s| s.as_node());
                if Self::is_modifier(statements, &kw) {
                    self.push("KEYWORD_UNTIL_MODIFIER", &kw, St::Set(BEG | LABEL));
                } else {
                    self.push("KEYWORD_UNTIL", &kw, St::Set(BEG));
                }
                self.push_opt("KEYWORD_DO_LOOP", n.do_keyword_loc(), St::Set(BEG));
                self.end_keyword(n.end_keyword_loc());
            }
            Node::InterpolatedMatchLastLineNode { .. } => {
                let n = node.as_interpolated_match_last_line_node().unwrap();
                self.regexp(n.opening_loc(), n.closing_loc());
            }
            Node::InterpolatedRegularExpressionNode { .. } => {
                let n = node.as_interpolated_regular_expression_node().unwrap();
                self.regexp(n.opening_loc(), n.closing_loc());
            }
            Node::MatchLastLineNode { .. } => {
                let n = node.as_match_last_line_node().unwrap();
                self.regexp(n.opening_loc(), n.closing_loc());
            }
            Node::RegularExpressionNode { .. } => {
                let n = node.as_regular_expression_node().unwrap();
                self.regexp(n.opening_loc(), n.closing_loc());
            }
            Node::InterpolatedStringNode { .. } => {
                let n = node.as_interpolated_string_node().unwrap();
                self.literal(n.opening_loc(), n.closing_loc(), Mode::Str);
            }
            Node::InterpolatedXStringNode { .. } => {
                let n = node.as_interpolated_x_string_node().unwrap();
                self.literal(Some(n.opening_loc()), Some(n.closing_loc()), Mode::Str);
            }
            Node::XStringNode { .. } => {
                let n = node.as_x_string_node().unwrap();
                self.literal(Some(n.opening_loc()), Some(n.closing_loc()), Mode::Str);
            }
            Node::InterpolatedSymbolNode { .. } => {
                let n = node.as_interpolated_symbol_node().unwrap();
                self.literal(n.opening_loc(), n.closing_loc(), Mode::Str);
            }
            Node::StringNode { .. } => {
                let n = node.as_string_node().unwrap();
                match n.opening_loc() {
                    Some(open) if self.text(&open) == b"?" => {
                        self.push("CHARACTER_LITERAL", &node.location(), St::Set(END));
                    }
                    open => self.literal(open, n.closing_loc(), Mode::Str),
                }
            }
            Node::SymbolNode { .. } => self.symbol_node(node),
            Node::KeywordRestParameterNode { .. } => {
                let n = node.as_keyword_rest_parameter_node().unwrap();
                self.push("USTAR_STAR", &n.operator_loc(), St::Set(BEG));
                self.push_opt("IDENTIFIER", n.name_loc(), St::Set(ARG));
            }
            Node::NoKeywordsParameterNode { .. } => {
                let n = node.as_no_keywords_parameter_node().unwrap();
                self.push("USTAR_STAR", &n.operator_loc(), St::Set(BEG));
                self.push("KEYWORD_NIL", &n.keyword_loc(), St::Set(END));
            }
            Node::RestParameterNode { .. } => {
                let n = node.as_rest_parameter_node().unwrap();
                self.push("USTAR", &n.operator_loc(), St::Set(BEG));
                self.push_opt("IDENTIFIER", n.name_loc(), St::Set(ARG));
            }
            Node::OptionalParameterNode { .. } => {
                let n = node.as_optional_parameter_node().unwrap();
                self.push("IDENTIFIER", &n.name_loc(), St::Set(ARG));
                self.push("EQUAL", &n.operator_loc(), St::Set(BEG));
            }
            Node::OptionalKeywordParameterNode { .. } => {
                let n = node.as_optional_keyword_parameter_node().unwrap();
                self.push("LABEL", &n.name_loc(), St::Set(ARG | LABELED));
            }
            Node::RequiredKeywordParameterNode { .. } => {
                let n = node.as_required_keyword_parameter_node().unwrap();
                self.push("LABEL", &n.name_loc(), St::Set(ARG | LABELED));
            }
            Node::MatchPredicateNode { .. } => {
                let n = node.as_match_predicate_node().unwrap();
                self.push("KEYWORD_IN", &n.keyword_loc(), St::Set(BEG | LABEL));
            }
            Node::MatchRequiredNode { .. } => {
                let n = node.as_match_required_node().unwrap();
                self.push("EQUAL_GREATER", &n.operator_loc(), St::Set(BEG | LABEL));
            }
            Node::MultiTargetNode { .. } => {
                let n = node.as_multi_target_node().unwrap();
                self.paren_open(n.lparen_loc());
                self.paren_close(n.rparen_loc());
            }
            Node::MultiWriteNode { .. } => {
                let n = node.as_multi_write_node().unwrap();
                self.paren_open(n.lparen_loc());
                self.paren_close(n.rparen_loc());
                self.push("EQUAL", &n.operator_loc(), St::Set(BEG));
            }
            Node::ParenthesesNode { .. } => {
                let n = node.as_parentheses_node().unwrap();
                let (open, _) = span(&n.opening_loc());
                self.push("PARENTHESIS_LEFT", &n.opening_loc(), St::Paren);
                self.push("PARENTHESIS_RIGHT", &n.closing_loc(), St::ParenClose(open));
            }
            Node::PinnedExpressionNode { .. } => {
                let n = node.as_pinned_expression_node().unwrap();
                self.push("CARET", &n.operator_loc(), St::Set(BEG));
                self.paren_open(Some(n.lparen_loc()));
                self.paren_close(Some(n.rparen_loc()));
            }
            Node::PinnedVariableNode { .. } => {
                let n = node.as_pinned_variable_node().unwrap();
                self.push("CARET", &n.operator_loc(), St::Set(BEG));
            }
            Node::PostExecutionNode { .. } => {
                let n = node.as_post_execution_node().unwrap();
                self.push("KEYWORD_END_UPCASE", &n.keyword_loc(), St::Set(END));
                self.push("BRACE_LEFT", &n.opening_loc(), St::Set(BEG));
                self.push("BRACE_RIGHT", &n.closing_loc(), St::Set(END));
            }
            Node::PreExecutionNode { .. } => {
                let n = node.as_pre_execution_node().unwrap();
                self.push("KEYWORD_BEGIN_UPCASE", &n.keyword_loc(), St::Set(END));
                self.push("BRACE_LEFT", &n.opening_loc(), St::Set(BEG));
                self.push("BRACE_RIGHT", &n.closing_loc(), St::Set(END));
            }
            Node::RescueModifierNode { .. } => {
                let n = node.as_rescue_modifier_node().unwrap();
                self.push(
                    "KEYWORD_RESCUE_MODIFIER",
                    &n.keyword_loc(),
                    St::Set(BEG | LABEL),
                );
            }
            Node::RescueNode { .. } => {
                let n = node.as_rescue_node().unwrap();
                self.keyword(Some(n.keyword_loc()), MID);
                self.push_opt("EQUAL_GREATER", n.operator_loc(), St::Set(BEG));
                self.keyword(n.then_keyword_loc(), BEG);
            }
            Node::SplatNode { .. } => {
                let n = node.as_splat_node().unwrap();
                self.push("USTAR", &n.operator_loc(), St::Set(BEG));
            }
            Node::UndefNode { .. } => {
                let n = node.as_undef_node().unwrap();
                self.keyword(Some(n.keyword_loc()), FNAME | FITEM);
            }
            _ => {}
        }
    }

    fn call_operator(&mut self, loc: Option<Location<'_>>) {
        if let Some(loc) = loc {
            let ty = match self.text(&loc) {
                b"&." => "AMPERSAND_DOT",
                b"::" => "COLON_COLON",
                _ => "DOT",
            };
            self.push(ty, &loc, St::Set(DOT));
        }
    }

    fn index_brackets(&mut self, open: &Location<'_>, close: &Location<'_>) {
        self.push("BRACKET_LEFT", open, St::Set(BEG | LABEL));
        self.push("BRACKET_RIGHT", close, St::Set(END));
    }

    fn block_delimiters(&mut self, open: Location<'_>, close: Location<'_>, brace: &'static str) {
        if self.text(&open) == b"{" {
            self.push(brace, &open, St::Set(BEG));
            self.push("BRACE_RIGHT", &close, St::Set(END));
        } else {
            self.push("KEYWORD_DO", &open, St::Set(BEG));
            self.end_keyword(Some(close));
        }
    }

    fn pattern_delimiters(
        &mut self,
        constant: bool,
        open: Option<Location<'_>>,
        close: Option<Location<'_>>,
    ) {
        if let Some(open) = open {
            match self.text(&open) {
                b"(" => self.paren_open(Some(open)),
                b"{" => self.push("BRACE_LEFT", &open, St::Set(BEG | LABEL)),
                _ if constant => self.push("BRACKET_LEFT", &open, St::Set(BEG | LABEL)),
                _ => self.push("BRACKET_LEFT_ARRAY", &open, St::Set(BEG | LABEL)),
            }
        }
        if let Some(close) = close {
            match self.text(&close) {
                b")" => self.paren_close(Some(close)),
                b"}" => self.push("BRACE_RIGHT", &close, St::Set(END)),
                _ => self.push("BRACKET_RIGHT", &close, St::Set(END)),
            }
        }
    }

    fn constant_path(&mut self, has_parent: bool, delimiter: &Location<'_>, name: &Location<'_>) {
        if has_parent {
            self.push("COLON_COLON", delimiter, St::Set(DOT));
        } else {
            self.push("UCOLON_COLON", delimiter, St::Set(BEG));
        }
        self.push("CONSTANT", name, St::Set(ARG));
    }

    fn range_operator(&mut self, has_left: bool, loc: &Location<'_>) {
        let dots3 = self.text(loc) == b"...";
        let ty = match (has_left, dots3) {
            (true, false) => "DOT_DOT",
            (true, true) => "DOT_DOT_DOT",
            (false, false) => "UDOT_DOT",
            (false, true) => "UDOT_DOT_DOT",
        };
        self.push(ty, loc, St::Set(BEG));
    }

    fn variable_write(&mut self, node: &Node<'_>, ty: &'static str, state: u32) {
        let (name, op) = match node {
            Node::ClassVariableWriteNode { .. } => {
                let n = node.as_class_variable_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::ClassVariableAndWriteNode { .. } => {
                let n = node.as_class_variable_and_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::ClassVariableOrWriteNode { .. } => {
                let n = node.as_class_variable_or_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::ClassVariableOperatorWriteNode { .. } => {
                let n = node.as_class_variable_operator_write_node().unwrap();
                (n.name_loc(), n.binary_operator_loc())
            }
            Node::InstanceVariableWriteNode { .. } => {
                let n = node.as_instance_variable_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::InstanceVariableAndWriteNode { .. } => {
                let n = node.as_instance_variable_and_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::InstanceVariableOrWriteNode { .. } => {
                let n = node.as_instance_variable_or_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::InstanceVariableOperatorWriteNode { .. } => {
                let n = node.as_instance_variable_operator_write_node().unwrap();
                (n.name_loc(), n.binary_operator_loc())
            }
            Node::GlobalVariableWriteNode { .. } => {
                let n = node.as_global_variable_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::GlobalVariableAndWriteNode { .. } => {
                let n = node.as_global_variable_and_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::GlobalVariableOrWriteNode { .. } => {
                let n = node.as_global_variable_or_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::GlobalVariableOperatorWriteNode { .. } => {
                let n = node.as_global_variable_operator_write_node().unwrap();
                (n.name_loc(), n.binary_operator_loc())
            }
            Node::LocalVariableWriteNode { .. } => {
                let n = node.as_local_variable_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::LocalVariableAndWriteNode { .. } => {
                let n = node.as_local_variable_and_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::LocalVariableOrWriteNode { .. } => {
                let n = node.as_local_variable_or_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::LocalVariableOperatorWriteNode { .. } => {
                let n = node.as_local_variable_operator_write_node().unwrap();
                (n.name_loc(), n.binary_operator_loc())
            }
            Node::ConstantWriteNode { .. } => {
                let n = node.as_constant_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::ConstantAndWriteNode { .. } => {
                let n = node.as_constant_and_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::ConstantOrWriteNode { .. } => {
                let n = node.as_constant_or_write_node().unwrap();
                (n.name_loc(), n.operator_loc())
            }
            Node::ConstantOperatorWriteNode { .. } => {
                let n = node.as_constant_operator_write_node().unwrap();
                (n.name_loc(), n.binary_operator_loc())
            }
            _ => return,
        };
        let ty = if ty == "GLOBAL_VARIABLE" {
            name_type(self.text(&name), false)
        } else {
            ty
        };
        self.push(ty, &name, St::Set(state));
        self.operator(Some(op), BEG);
    }

    fn call_node(&mut self, node: &Node<'_>) {
        let n = node.as_call_node().unwrap();
        let name = n.name().as_slice();
        let receiver = n.receiver();
        self.call_operator(n.call_operator_loc());
        let opening = n.opening_loc();
        let is_index = (name == b"[]" || name == b"[]=")
            && opening.as_ref().is_some_and(|o| self.text(o) == b"[");
        if is_index {
            self.index_brackets(&opening.unwrap(), &n.closing_loc().unwrap());
            self.push_opt("EQUAL", n.equal_loc(), St::Set(BEG));
            return;
        }
        if let Some(msg) = n.message_loc() {
            let text = self.text(&msg);
            let after_dot = n.call_operator_loc().is_some();
            if text == b"not" && !after_dot {
                self.push("KEYWORD_NOT", &msg, St::Set(ARG));
            } else if !after_dot && receiver.is_some() && operator_type(text).is_some() {
                let (end, unary) = (
                    msg.end() as usize,
                    name.ends_with(b"@") || name == b"!" || name == b"~",
                );
                let ty = match text {
                    b"-" if unary => {
                        if self.src.get(end).is_some_and(u8::is_ascii_digit) {
                            "UMINUS_NUM"
                        } else {
                            "UMINUS"
                        }
                    }
                    b"+" if unary => "UPLUS",
                    _ => operator_type(text).unwrap(),
                };
                self.push(ty, &msg, St::Set(BEG));
            } else {
                self.name(&msg, false, ARG);
            }
        }
        if let Some(open) = opening {
            self.paren_open(Some(open));
        }
        self.paren_close(n.closing_loc());
        self.push_opt("EQUAL", n.equal_loc(), St::Set(BEG));
    }

    fn symbol_node(&mut self, node: &Node<'_>) {
        let n = node.as_symbol_node().unwrap();
        let (open, value, close) = (n.opening_loc(), Some(n.content_loc()), n.closing_loc());
        match (open, close) {
            (None, None) => {
                if !self.in_array()
                    && let Some(value) = value
                {
                    self.name(&value, true, ENDFN);
                }
            }
            (None, Some(close)) => {
                let start = value.map_or(close.start(), |v| v.start()) as usize;
                self.toks.push(Tok::new(
                    "LABEL",
                    start,
                    close.end() as usize,
                    St::Set(ARG | LABELED),
                ));
            }
            (Some(open), None) if self.text(&open) == b":" => {
                self.push("SYMBOL_BEGIN", &open, St::Set(FNAME));
                if let Some(value) = value {
                    self.name(&value, true, ENDFN);
                }
            }
            (open, close) => self.literal(open, close, Mode::Str),
        }
    }
}

macro_rules! visit_all {
    ($($method:ident: $ty:ident,)*) => {
        $(
            fn $method(&mut self, node: &ruby_prism::$ty<'pr>) {
                let generic = node.as_node();
                self.enter(&generic);
                self.parents.push(matches!(generic, Node::ArrayNode { .. }));
                match &generic {
                    // The value repeats the key's location (`{a:}`).
                    Node::ImplicitNode { .. } => {}
                    // The targets are the regexp's named captures.
                    Node::MatchWriteNode { .. } => {
                        let n = generic.as_match_write_node().unwrap();
                        self.visit(&n.call().as_node());
                    }
                    // The numeric is part of the imaginary literal's token.
                    Node::ImaginaryNode { .. } => {}
                    _ => ruby_prism::$method(self, node),
                }
                self.parents.pop();
            }
        )*
    };
}

impl<'pr> Visit<'pr> for Collector<'_, 'pr> {
    visit_all! {
        visit_alias_global_variable_node: AliasGlobalVariableNode,
        visit_alias_method_node: AliasMethodNode,
        visit_alternation_pattern_node: AlternationPatternNode,
        visit_and_node: AndNode,
        visit_arguments_node: ArgumentsNode,
        visit_array_node: ArrayNode,
        visit_array_pattern_node: ArrayPatternNode,
        visit_assoc_node: AssocNode,
        visit_assoc_splat_node: AssocSplatNode,
        visit_back_reference_read_node: BackReferenceReadNode,
        visit_begin_node: BeginNode,
        visit_block_argument_node: BlockArgumentNode,
        visit_block_local_variable_node: BlockLocalVariableNode,
        visit_block_node: BlockNode,
        visit_block_parameter_node: BlockParameterNode,
        visit_block_parameters_node: BlockParametersNode,
        visit_break_node: BreakNode,
        visit_call_and_write_node: CallAndWriteNode,
        visit_call_node: CallNode,
        visit_call_operator_write_node: CallOperatorWriteNode,
        visit_call_or_write_node: CallOrWriteNode,
        visit_call_target_node: CallTargetNode,
        visit_capture_pattern_node: CapturePatternNode,
        visit_case_match_node: CaseMatchNode,
        visit_case_node: CaseNode,
        visit_class_node: ClassNode,
        visit_class_variable_and_write_node: ClassVariableAndWriteNode,
        visit_class_variable_operator_write_node: ClassVariableOperatorWriteNode,
        visit_class_variable_or_write_node: ClassVariableOrWriteNode,
        visit_class_variable_read_node: ClassVariableReadNode,
        visit_class_variable_target_node: ClassVariableTargetNode,
        visit_class_variable_write_node: ClassVariableWriteNode,
        visit_constant_and_write_node: ConstantAndWriteNode,
        visit_constant_operator_write_node: ConstantOperatorWriteNode,
        visit_constant_or_write_node: ConstantOrWriteNode,
        visit_constant_path_and_write_node: ConstantPathAndWriteNode,
        visit_constant_path_node: ConstantPathNode,
        visit_constant_path_operator_write_node: ConstantPathOperatorWriteNode,
        visit_constant_path_or_write_node: ConstantPathOrWriteNode,
        visit_constant_path_target_node: ConstantPathTargetNode,
        visit_constant_path_write_node: ConstantPathWriteNode,
        visit_constant_read_node: ConstantReadNode,
        visit_constant_target_node: ConstantTargetNode,
        visit_constant_write_node: ConstantWriteNode,
        visit_def_node: DefNode,
        visit_defined_node: DefinedNode,
        visit_else_node: ElseNode,
        visit_embedded_statements_node: EmbeddedStatementsNode,
        visit_embedded_variable_node: EmbeddedVariableNode,
        visit_ensure_node: EnsureNode,
        visit_error_recovery_node: ErrorRecoveryNode,
        visit_false_node: FalseNode,
        visit_find_pattern_node: FindPatternNode,
        visit_flip_flop_node: FlipFlopNode,
        visit_float_node: FloatNode,
        visit_for_node: ForNode,
        visit_forwarding_arguments_node: ForwardingArgumentsNode,
        visit_forwarding_parameter_node: ForwardingParameterNode,
        visit_forwarding_super_node: ForwardingSuperNode,
        visit_global_variable_and_write_node: GlobalVariableAndWriteNode,
        visit_global_variable_operator_write_node: GlobalVariableOperatorWriteNode,
        visit_global_variable_or_write_node: GlobalVariableOrWriteNode,
        visit_global_variable_read_node: GlobalVariableReadNode,
        visit_global_variable_target_node: GlobalVariableTargetNode,
        visit_global_variable_write_node: GlobalVariableWriteNode,
        visit_hash_node: HashNode,
        visit_hash_pattern_node: HashPatternNode,
        visit_if_node: IfNode,
        visit_imaginary_node: ImaginaryNode,
        visit_implicit_node: ImplicitNode,
        visit_implicit_rest_node: ImplicitRestNode,
        visit_in_node: InNode,
        visit_index_and_write_node: IndexAndWriteNode,
        visit_index_operator_write_node: IndexOperatorWriteNode,
        visit_index_or_write_node: IndexOrWriteNode,
        visit_index_target_node: IndexTargetNode,
        visit_instance_variable_and_write_node: InstanceVariableAndWriteNode,
        visit_instance_variable_operator_write_node: InstanceVariableOperatorWriteNode,
        visit_instance_variable_or_write_node: InstanceVariableOrWriteNode,
        visit_instance_variable_read_node: InstanceVariableReadNode,
        visit_instance_variable_target_node: InstanceVariableTargetNode,
        visit_instance_variable_write_node: InstanceVariableWriteNode,
        visit_integer_node: IntegerNode,
        visit_interpolated_match_last_line_node: InterpolatedMatchLastLineNode,
        visit_interpolated_regular_expression_node: InterpolatedRegularExpressionNode,
        visit_interpolated_string_node: InterpolatedStringNode,
        visit_interpolated_symbol_node: InterpolatedSymbolNode,
        visit_interpolated_x_string_node: InterpolatedXStringNode,
        visit_it_local_variable_read_node: ItLocalVariableReadNode,
        visit_it_parameters_node: ItParametersNode,
        visit_keyword_hash_node: KeywordHashNode,
        visit_keyword_rest_parameter_node: KeywordRestParameterNode,
        visit_lambda_node: LambdaNode,
        visit_local_variable_and_write_node: LocalVariableAndWriteNode,
        visit_local_variable_operator_write_node: LocalVariableOperatorWriteNode,
        visit_local_variable_or_write_node: LocalVariableOrWriteNode,
        visit_local_variable_read_node: LocalVariableReadNode,
        visit_local_variable_target_node: LocalVariableTargetNode,
        visit_local_variable_write_node: LocalVariableWriteNode,
        visit_match_last_line_node: MatchLastLineNode,
        visit_match_predicate_node: MatchPredicateNode,
        visit_match_required_node: MatchRequiredNode,
        visit_match_write_node: MatchWriteNode,
        visit_module_node: ModuleNode,
        visit_multi_target_node: MultiTargetNode,
        visit_multi_write_node: MultiWriteNode,
        visit_next_node: NextNode,
        visit_nil_node: NilNode,
        visit_no_block_parameter_node: NoBlockParameterNode,
        visit_no_keywords_parameter_node: NoKeywordsParameterNode,
        visit_numbered_parameters_node: NumberedParametersNode,
        visit_numbered_reference_read_node: NumberedReferenceReadNode,
        visit_optional_keyword_parameter_node: OptionalKeywordParameterNode,
        visit_optional_parameter_node: OptionalParameterNode,
        visit_or_node: OrNode,
        visit_parameters_node: ParametersNode,
        visit_parentheses_node: ParenthesesNode,
        visit_pinned_expression_node: PinnedExpressionNode,
        visit_pinned_variable_node: PinnedVariableNode,
        visit_post_execution_node: PostExecutionNode,
        visit_pre_execution_node: PreExecutionNode,
        visit_program_node: ProgramNode,
        visit_range_node: RangeNode,
        visit_rational_node: RationalNode,
        visit_redo_node: RedoNode,
        visit_regular_expression_node: RegularExpressionNode,
        visit_required_keyword_parameter_node: RequiredKeywordParameterNode,
        visit_required_parameter_node: RequiredParameterNode,
        visit_rescue_modifier_node: RescueModifierNode,
        visit_rescue_node: RescueNode,
        visit_rest_parameter_node: RestParameterNode,
        visit_retry_node: RetryNode,
        visit_return_node: ReturnNode,
        visit_self_node: SelfNode,
        visit_shareable_constant_node: ShareableConstantNode,
        visit_singleton_class_node: SingletonClassNode,
        visit_source_encoding_node: SourceEncodingNode,
        visit_source_file_node: SourceFileNode,
        visit_source_line_node: SourceLineNode,
        visit_splat_node: SplatNode,
        visit_statements_node: StatementsNode,
        visit_string_node: StringNode,
        visit_super_node: SuperNode,
        visit_symbol_node: SymbolNode,
        visit_true_node: TrueNode,
        visit_undef_node: UndefNode,
        visit_unless_node: UnlessNode,
        visit_until_node: UntilNode,
        visit_when_node: WhenNode,
        visit_while_node: WhileNode,
        visit_x_string_node: XStringNode,
        visit_yield_node: YieldNode,
    }
}

/// Emits the collected tokens in Prism's lexing order, filling the gaps.
struct Emitter<'s> {
    src: &'s [u8],
    toks: Vec<Tok>,
    heredocs: Vec<Heredoc>,
    /// Per heredoc: where its body starts.
    body_start: Vec<usize>,
    /// Token indices per group: 0 is the main stream, `h + 1` heredoc `h`.
    groups: Vec<Vec<usize>>,
    /// Heredoc bodies directly inside each group, by body start.
    children: Vec<Vec<usize>>,
    ruby4: bool,
    out: Vec<RawToken>,
    unknown: Vec<usize>,
    state: u32,
    space_seen: bool,
    ctx: Vec<Mode>,
    paren_types: HashMap<usize, &'static str>,
}

impl<'s> Emitter<'s> {
    fn new(src: &'s [u8], toks: Vec<Tok>, heredocs: Vec<Heredoc>, ruby4: bool) -> Self {
        let mut emitter = Self {
            src,
            toks,
            heredocs,
            body_start: Vec::new(),
            groups: Vec::new(),
            children: Vec::new(),
            ruby4,
            out: Vec::new(),
            unknown: Vec::new(),
            state: BEG,
            space_seen: false,
            ctx: Vec::new(),
            paren_types: HashMap::new(),
        };
        emitter.layout();
        emitter
    }

    /// Works out heredoc body regions and which tokens each one owns.
    fn layout(&mut self) {
        let mut order: Vec<usize> = (0..self.heredocs.len()).collect();
        order.sort_by_key(|&h| self.heredocs[h].open_end);
        self.body_start = vec![0; self.heredocs.len()];
        let mut chained: HashMap<usize, usize> = HashMap::new();
        for &h in &order {
            let heredoc = self.heredocs[h];
            let newline = self.src[heredoc.open_end..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(self.src.len(), |i| heredoc.open_end + i);
            let start = chained
                .get(&newline)
                .copied()
                .unwrap_or((newline + 1).min(self.src.len()));
            self.body_start[h] = start.min(heredoc.close_start);
            chained.insert(newline, heredoc.close_end);
        }

        // Regions nest or are disjoint: the innermost one containing a
        // position owns it.
        let regions: Vec<(usize, usize, usize)> = {
            let mut r: Vec<(usize, usize, usize)> = (0..self.heredocs.len())
                .map(|h| (self.body_start[h], self.heredocs[h].close_end, h))
                .collect();
            r.sort_by_key(|&(start, end, _)| (start, std::cmp::Reverse(end)));
            r
        };
        // Tokens and regions are sorted by start, so one sweep with a stack
        // of the open regions finds each one's innermost owner.
        self.groups = vec![Vec::new(); self.heredocs.len() + 1];
        self.children = vec![Vec::new(); self.heredocs.len() + 1];
        let mut open: Vec<(usize, usize)> = Vec::new();
        let mut next = 0;
        for (i, tok) in self.toks.iter().enumerate() {
            while let Some(&(start, end, h)) = regions.get(next)
                && start <= tok.start
            {
                while open.last().is_some_and(|&(e, _)| e <= start) {
                    open.pop();
                }
                self.children[open.last().map_or(0, |&(_, o)| o + 1)].push(h);
                open.push((end, h));
                next += 1;
            }
            while open.last().is_some_and(|&(e, _)| e <= tok.start) {
                open.pop();
            }
            self.groups[open.last().map_or(0, |&(_, o)| o + 1)].push(i);
        }
        for &(start, end, h) in &regions[next..] {
            while open.last().is_some_and(|&(e, _)| e <= start) {
                open.pop();
            }
            self.children[open.last().map_or(0, |&(_, o)| o + 1)].push(h);
            open.push((end, h));
        }
    }

    fn run(&mut self, end: usize) {
        self.emit_group(0, 0, end, Mode::Code);
    }

    fn emit_group(&mut self, group: usize, start: usize, end: usize, mode: Mode) {
        let saved = std::mem::replace(&mut self.ctx, vec![mode]);
        let mut pos = start;
        let indices = self.groups[group].clone();
        for i in indices {
            let tok = self.toks[i];
            if tok.start < pos {
                continue;
            }
            self.fill(group, pos, tok.start);
            self.emit(tok);
            pos = tok.end;
            if let Some(h) = tok.heredoc {
                let heredoc = self.heredocs[h];
                let mode = if heredoc.squiggly {
                    Mode::Squiggly
                } else {
                    Mode::Heredoc
                };
                self.emit_group(h + 1, self.body_start[h], heredoc.close_end, mode);
            }
        }
        self.fill(group, pos, end);
        self.ctx = saved;
    }

    /// The heredoc body of `group` that starts at `pos`, if any.
    fn child_at(&self, group: usize, pos: usize) -> Option<usize> {
        let children = &self.children[group];
        let i = children.partition_point(|&h| self.body_start[h] < pos);
        children
            .get(i)
            .copied()
            .filter(|&h| self.body_start[h] == pos)
    }

    /// Where the next heredoc body of `group` at or after `pos` starts.
    fn next_child_start(&self, group: usize, pos: usize) -> Option<usize> {
        let children = &self.children[group];
        let i = children.partition_point(|&h| self.body_start[h] < pos);
        children.get(i).map(|&h| self.body_start[h])
    }

    /// Skips the heredoc bodies that start at `pos`.
    fn skip_bodies(&self, group: usize, mut pos: usize) -> usize {
        while let Some(h) = self.child_at(group, pos) {
            let end = self.heredocs[h].close_end;
            if end <= pos {
                break;
            }
            pos = end;
        }
        pos
    }

    fn fill(&mut self, group: usize, from: usize, to: usize) {
        let mut pos = from;
        while pos < to {
            let skipped = self.skip_bodies(group, pos);
            if skipped != pos {
                pos = skipped;
                continue;
            }
            let mode = *self.ctx.last().unwrap_or(&Mode::Code);
            if mode == Mode::Code {
                pos = self.scan_code(group, pos, to);
            } else {
                let stop = self
                    .next_child_start(group, pos + 1)
                    .map_or(to, |s| s.min(to));
                self.content(mode, pos, stop);
                pos = stop;
            }
        }
    }

    fn push_raw(&mut self, ty: &'static str, start: usize, end: usize) {
        self.out.push(RawToken {
            ty,
            start,
            end,
            state: self.state,
        });
    }

    fn content(&mut self, mode: Mode, from: usize, to: usize) {
        match mode {
            Mode::Code => {}
            Mode::Str | Mode::Heredoc => self.push_raw("STRING_CONTENT", from, to),
            Mode::Squiggly => {
                let mut start = from;
                while start < to {
                    let end = self.src[start..to]
                        .iter()
                        .position(|&b| b == b'\n')
                        .map_or(to, |i| start + i + 1);
                    self.push_raw("STRING_CONTENT", start, end);
                    start = end;
                }
            }
            Mode::List => {
                let mut pos = from;
                while pos < to {
                    let start = pos;
                    if is_space(self.src[pos]) {
                        while pos < to && is_space(self.src[pos]) {
                            pos += 1;
                        }
                        self.push_raw("WORDS_SEP", start, pos);
                    } else {
                        while pos < to && !is_space(self.src[pos]) {
                            pos += if self.src[pos] == b'\\' { 2 } else { 1 };
                        }
                        pos = pos.min(to);
                        self.push_raw("STRING_CONTENT", start, pos);
                    }
                }
            }
        }
    }

    /// Scans one item of code-gap text at `pos`; returns the next position.
    fn scan_code(&mut self, group: usize, pos: usize, to: usize) -> usize {
        match self.src[pos] {
            b' ' | b'\t' | b'\x0c' | b'\x0b' => {
                self.space_seen = true;
                pos + 1
            }
            b'\r' => pos + 1,
            b'\\' if self.src.get(pos + 1) == Some(&b'\n') => {
                self.space_seen = true;
                pos + 2
            }
            b'\\'
                if self.src.get(pos + 1) == Some(&b'\r')
                    && self.src.get(pos + 2) == Some(&b'\n') =>
            {
                self.space_seen = true;
                pos + 3
            }
            b'\n' => {
                let ty = self.newline(group, pos);
                self.push_raw(ty, pos, pos + 1);
                if ty == "NEWLINE" {
                    self.space_seen = false;
                }
                pos + 1
            }
            b',' => {
                self.state = BEG | LABEL;
                self.push_raw("COMMA", pos, pos + 1);
                self.space_seen = false;
                pos + 1
            }
            b';' => {
                self.state = BEG;
                self.push_raw("SEMICOLON", pos, pos + 1);
                self.space_seen = false;
                pos + 1
            }
            _ => {
                self.unknown.push(pos);
                let _ = to;
                pos + 1
            }
        }
    }

    /// The type of the newline at `pos` (`NEWLINE` or `IGNORED_NEWLINE`),
    /// updating the lexer state as Prism's lexer does.
    fn newline(&mut self, group: usize, pos: usize) -> &'static str {
        let state = self.state;
        let ignored = (state & (BEG | CLASS | FNAME | DOT) != 0 && state & LABELED == 0)
            || (state & !LABEL) == (ARG | LABELED);
        if ignored {
            return "IGNORED_NEWLINE";
        }
        let src = self.src;
        let at = |i: usize| src.get(i).copied().unwrap_or(0);
        let mut next = self.skip_bodies(group, pos + 1);
        while matches!(at(next), b' ' | b'\t' | b'\x0c' | b'\x0b' | b'\r') {
            next += 1;
        }
        let continues = |i: usize, ruby4: bool| -> bool {
            (at(i) == b'.' && at(i + 1) != b'.')
                || (at(i) == b'&' && at(i + 1) == b'.')
                || (ruby4
                    && ((at(i) == b'&' && at(i + 1) == b'&')
                        || (at(i) == b'|' && at(i + 1) == b'|')
                        || (src[i.min(src.len())..].starts_with(b"and")
                            && !is_ident_byte(at(i + 3)))
                        || (src[i.min(src.len())..].starts_with(b"or")
                            && !is_ident_byte(at(i + 2)))))
        };
        if at(next) == b'#' {
            // Look past comment lines for a leading `.` or `&.`.
            let mut following = next;
            loop {
                let Some(nl) = src[following..].iter().position(|&b| b == b'\n') else {
                    following = src.len();
                    break;
                };
                following += nl + 1;
                while matches!(at(following), b' ' | b'\t' | b'\x0c' | b'\x0b' | b'\r') {
                    following += 1;
                }
                if at(following) != b'#' {
                    break;
                }
            }
            if following < src.len()
                && ((at(following) == b'.')
                    || (at(following) == b'&' && at(following + 1) == b'.')
                    || (self.ruby4 && continues(following, true)))
            {
                return "IGNORED_NEWLINE";
            }
        }
        if at(next) == b'.' && at(next + 1) == b'.' {
            self.state = BEG;
            return "IGNORED_NEWLINE";
        }
        if continues(next, self.ruby4) {
            return "IGNORED_NEWLINE";
        }
        self.state = BEG;
        "NEWLINE"
    }

    fn emit(&mut self, tok: Tok) {
        let mut ty = tok.ty;
        let before = self.state;
        match tok.st {
            St::Keep => {}
            St::Set(state) => self.state = state,
            St::Paren => {
                if self.space_seen && (before & (ARG | CMDARG) != 0 || before == (END | LABEL)) {
                    ty = "PARENTHESIS_LEFT_PARENTHESES";
                }
                self.paren_types.insert(tok.start, ty);
                self.state = BEG | LABEL;
            }
            St::ParenClose(open) => {
                self.state = if self.paren_types.get(&open) == Some(&"PARENTHESIS_LEFT_PARENTHESES")
                {
                    ENDARG
                } else {
                    ENDFN
                };
            }
            St::Comment => {}
        }
        self.push_raw(ty, tok.start, tok.end);
        if let St::Comment = tok.st
            && self.src[..tok.end].ends_with(b"\n")
        {
            // The comment's newline: no token, but it moves the state.
            let group = 0;
            let _ = self.newline(group, tok.end - 1);
        }
        match tok.ctx {
            Ctx::None => {}
            Ctx::Push(mode) => self.ctx.push(mode),
            Ctx::Pop => {
                self.ctx.pop();
            }
        }
        self.space_seen = false;
    }
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' | b'\x0b')
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn keyword_type(text: &[u8]) -> Option<&'static str> {
    Some(match text {
        b"alias" => "KEYWORD_ALIAS",
        b"and" => "KEYWORD_AND",
        b"begin" => "KEYWORD_BEGIN",
        b"BEGIN" => "KEYWORD_BEGIN_UPCASE",
        b"break" => "KEYWORD_BREAK",
        b"case" => "KEYWORD_CASE",
        b"class" => "KEYWORD_CLASS",
        b"def" => "KEYWORD_DEF",
        b"defined?" => "KEYWORD_DEFINED",
        b"do" => "KEYWORD_DO",
        b"else" => "KEYWORD_ELSE",
        b"elsif" => "KEYWORD_ELSIF",
        b"end" => "KEYWORD_END",
        b"END" => "KEYWORD_END_UPCASE",
        b"ensure" => "KEYWORD_ENSURE",
        b"false" => "KEYWORD_FALSE",
        b"for" => "KEYWORD_FOR",
        b"if" => "KEYWORD_IF",
        b"in" => "KEYWORD_IN",
        b"module" => "KEYWORD_MODULE",
        b"next" => "KEYWORD_NEXT",
        b"nil" => "KEYWORD_NIL",
        b"not" => "KEYWORD_NOT",
        b"or" => "KEYWORD_OR",
        b"redo" => "KEYWORD_REDO",
        b"rescue" => "KEYWORD_RESCUE",
        b"retry" => "KEYWORD_RETRY",
        b"return" => "KEYWORD_RETURN",
        b"self" => "KEYWORD_SELF",
        b"super" => "KEYWORD_SUPER",
        b"then" => "KEYWORD_THEN",
        b"true" => "KEYWORD_TRUE",
        b"undef" => "KEYWORD_UNDEF",
        b"unless" => "KEYWORD_UNLESS",
        b"until" => "KEYWORD_UNTIL",
        b"when" => "KEYWORD_WHEN",
        b"while" => "KEYWORD_WHILE",
        b"yield" => "KEYWORD_YIELD",
        b"__ENCODING__" => "KEYWORD___ENCODING__",
        b"__FILE__" => "KEYWORD___FILE__",
        b"__LINE__" => "KEYWORD___LINE__",
        _ => return None,
    })
}

fn operator_type(text: &[u8]) -> Option<&'static str> {
    Some(match text {
        b"!" | b"!@" => "BANG",
        b"!=" => "BANG_EQUAL",
        b"!~" => "BANG_TILDE",
        b"%" => "PERCENT",
        b"%=" => "PERCENT_EQUAL",
        b"&" => "AMPERSAND",
        b"&&" => "AMPERSAND_AMPERSAND",
        b"&&=" => "AMPERSAND_AMPERSAND_EQUAL",
        b"&=" => "AMPERSAND_EQUAL",
        b"*" => "STAR",
        b"**" => "STAR_STAR",
        b"**=" => "STAR_STAR_EQUAL",
        b"*=" => "STAR_EQUAL",
        b"+" => "PLUS",
        b"+=" => "PLUS_EQUAL",
        b"+@" => "UPLUS",
        b"-" => "MINUS",
        b"-=" => "MINUS_EQUAL",
        b"-@" => "UMINUS",
        b"/" => "SLASH",
        b"/=" => "SLASH_EQUAL",
        b"<" => "LESS",
        b"<<" => "LESS_LESS",
        b"<<=" => "LESS_LESS_EQUAL",
        b"<=" => "LESS_EQUAL",
        b"<=>" => "LESS_EQUAL_GREATER",
        b"=" => "EQUAL",
        b"==" => "EQUAL_EQUAL",
        b"===" => "EQUAL_EQUAL_EQUAL",
        b"=~" => "EQUAL_TILDE",
        b">" => "GREATER",
        b">=" => "GREATER_EQUAL",
        b">>" => "GREATER_GREATER",
        b">>=" => "GREATER_GREATER_EQUAL",
        b"^" => "CARET",
        b"^=" => "CARET_EQUAL",
        b"|" => "PIPE",
        b"|=" => "PIPE_EQUAL",
        b"||" => "PIPE_PIPE",
        b"||=" => "PIPE_PIPE_EQUAL",
        b"~" | b"~@" => "TILDE",
        b"[]" => "BRACKET_LEFT_RIGHT",
        b"[]=" => "BRACKET_LEFT_RIGHT_EQUAL",
        b"`" => "BACKTICK",
        b"and" => "KEYWORD_AND",
        b"or" => "KEYWORD_OR",
        _ => return None,
    })
}

/// The token type of a name (method, symbol, variable) as Prism lexes it.
fn name_type(text: &[u8], keywords: bool) -> &'static str {
    match text.first() {
        Some(b'@') if text.get(1) == Some(&b'@') => return "CLASS_VARIABLE",
        Some(b'@') => return "INSTANCE_VARIABLE",
        // In a symbol or method-name position the lexer is in `FNAME`,
        // where back and numbered references lex as plain globals.
        Some(b'$') if keywords => return "GLOBAL_VARIABLE",
        Some(b'$') => {
            return match text.get(1) {
                Some(b'&' | b'`' | b'\'' | b'+') if text.len() == 2 => "BACK_REFERENCE",
                Some(b'1'..=b'9') => "NUMBERED_REFERENCE",
                _ => "GLOBAL_VARIABLE",
            };
        }
        _ => {}
    }
    if text != b"and"
        && text != b"or"
        && let Some(ty) = operator_type(text)
    {
        return ty;
    }
    if keywords && let Some(ty) = keyword_type(text) {
        return ty;
    }
    if text.ends_with(b"?") || text.ends_with(b"!") {
        return "METHOD_NAME";
    }
    let first = std::str::from_utf8(text)
        .ok()
        .and_then(|s| s.chars().next());
    if first.is_some_and(char::is_uppercase) && !text.ends_with(b"=") {
        "CONSTANT"
    } else {
        "IDENTIFIER"
    }
}

fn number_type(text: &[u8]) -> &'static str {
    let lower: Vec<u8> = text.to_ascii_lowercase();
    let based = lower.starts_with(b"0x")
        || lower.starts_with(b"0b")
        || lower.starts_with(b"0o")
        || lower.starts_with(b"0d");
    let (body, imaginary) = match lower.strip_suffix(b"i") {
        Some(rest) => (rest, true),
        None => (lower.as_slice(), false),
    };
    let (body, rational) = match body.strip_suffix(b"r") {
        Some(rest) => (rest, true),
        None => (body, false),
    };
    let float = !based && (body.contains(&b'.') || body.contains(&b'e'));
    match (float, rational, imaginary) {
        (false, false, false) => "INTEGER",
        (false, true, false) => "INTEGER_RATIONAL",
        (false, false, true) => "INTEGER_IMAGINARY",
        (false, true, true) => "INTEGER_RATIONAL_IMAGINARY",
        (true, false, false) => "FLOAT",
        (true, true, false) => "FLOAT_RATIONAL",
        (true, false, true) => "FLOAT_IMAGINARY",
        (true, true, true) => "FLOAT_RATIONAL_IMAGINARY",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// The stream as `TYPE start..end;` items, and the unclassified gaps.
    fn stream(source: &str) -> (String, Vec<usize>) {
        let ps = ProcessedSource::new(source, source, 3.3, Path::new("t.rb"));
        let (tokens, unknown) = prism_tokens_checked(&ps);
        let text = tokens
            .iter()
            .map(|t| format!("{} {}..{};", t.ty, t.start, t.end))
            .collect();
        (text, unknown)
    }

    // The expectations are `script/dump_prism_tokens` output (Prism.lex).
    #[test]
    fn heredocs_and_comments() {
        let (text, unknown) = stream(
            "foo(<<~A, <<-B) # c\n  x #{y}\n    z\nA\n  b\n  B\nbar\n  # note\n  .baz&.qux\n",
        );
        assert_eq!(
            text,
            "IDENTIFIER 0..3;PARENTHESIS_LEFT 3..4;HEREDOC_START 4..8;STRING_CONTENT 20..24;EMBEXPR_BEGIN 24..26;IDENTIFIER 26..27;EMBEXPR_END 27..28;STRING_CONTENT 28..29;STRING_CONTENT 29..35;HEREDOC_END 35..37;COMMA 8..9;HEREDOC_START 10..14;STRING_CONTENT 37..41;HEREDOC_END 41..45;PARENTHESIS_RIGHT 14..15;COMMENT 16..20;IDENTIFIER 45..48;IGNORED_NEWLINE 48..49;COMMENT 51..58;DOT 60..61;IDENTIFIER 61..64;AMPERSAND_DOT 64..66;IDENTIFIER 66..69;NEWLINE 69..70;"
        );
        assert!(unknown.is_empty());
    }

    #[test]
    fn parameters_literals_and_patterns() {
        let (text, unknown) = stream(
            "def foo(a, *b, c: 1, **d, &e)\n  x = %w[ p q\\ r ]\n  puts (1) + -2\n  h = { a:, \"b\": 1, c => :$&, d: :end }\n  case h\n  in {a: [*, 1 | 2 => y, *]} if y\n    ->(z) { z ? 1 : 2 }\n  end\nend\n=begin\ndoc\n=end\n__END__\ndata\n",
        );
        assert_eq!(
            text,
            "KEYWORD_DEF 0..3;IDENTIFIER 4..7;PARENTHESIS_LEFT 7..8;IDENTIFIER 8..9;COMMA 9..10;USTAR 11..12;IDENTIFIER 12..13;COMMA 13..14;LABEL 15..17;INTEGER 18..19;COMMA 19..20;USTAR_STAR 21..23;IDENTIFIER 23..24;COMMA 24..25;UAMPERSAND 26..27;IDENTIFIER 27..28;PARENTHESIS_RIGHT 28..29;IGNORED_NEWLINE 29..30;IDENTIFIER 32..33;EQUAL 34..35;PERCENT_LOWER_W 36..39;WORDS_SEP 39..40;STRING_CONTENT 40..41;WORDS_SEP 41..42;STRING_CONTENT 42..46;WORDS_SEP 46..47;STRING_END 47..48;NEWLINE 48..49;IDENTIFIER 51..55;PARENTHESIS_LEFT_PARENTHESES 56..57;INTEGER 57..58;PARENTHESIS_RIGHT 58..59;PLUS 60..61;UMINUS_NUM 62..63;INTEGER 63..64;NEWLINE 64..65;IDENTIFIER 67..68;EQUAL 69..70;BRACE_LEFT 71..72;LABEL 73..75;COMMA 75..76;STRING_BEGIN 77..78;STRING_CONTENT 78..79;LABEL_END 79..81;INTEGER 82..83;COMMA 83..84;IDENTIFIER 85..86;EQUAL_GREATER 87..89;SYMBOL_BEGIN 90..91;GLOBAL_VARIABLE 91..93;COMMA 93..94;LABEL 95..97;SYMBOL_BEGIN 98..99;KEYWORD_END 99..102;BRACE_RIGHT 103..104;NEWLINE 104..105;KEYWORD_CASE 107..111;IDENTIFIER 112..113;NEWLINE 113..114;KEYWORD_IN 116..118;BRACE_LEFT 119..120;LABEL 120..122;BRACKET_LEFT_ARRAY 123..124;USTAR 124..125;COMMA 125..126;INTEGER 127..128;PIPE 129..130;INTEGER 131..132;EQUAL_GREATER 133..135;IDENTIFIER 136..137;COMMA 137..138;USTAR 139..140;BRACKET_RIGHT 140..141;BRACE_RIGHT 141..142;KEYWORD_IF_MODIFIER 143..145;IDENTIFIER 146..147;NEWLINE 147..148;MINUS_GREATER 152..154;PARENTHESIS_LEFT 154..155;IDENTIFIER 155..156;PARENTHESIS_RIGHT 156..157;LAMBDA_BEGIN 158..159;IDENTIFIER 160..161;QUESTION_MARK 162..163;INTEGER 164..165;COLON 166..167;INTEGER 168..169;BRACE_RIGHT 170..171;NEWLINE 171..172;KEYWORD_END 174..177;NEWLINE 177..178;KEYWORD_END 178..181;NEWLINE 181..182;EMBDOC_BEGIN 182..189;EMBDOC_LINE 189..193;EMBDOC_END 193..198;__END__ 198..211;"
        );
        assert!(unknown.is_empty());
    }

    #[test]
    fn paren_less_def_and_else_before_ensure() {
        let (text, unknown) = stream(
            "def each &block\n  begin\n    x\n  rescue E\n  else\n    y\n  ensure\n    z\n  end\nend\n",
        );
        assert_eq!(
            text,
            "KEYWORD_DEF 0..3;IDENTIFIER 4..8;AMPERSAND 9..10;IDENTIFIER 10..15;NEWLINE 15..16;KEYWORD_BEGIN 18..23;IGNORED_NEWLINE 23..24;IDENTIFIER 28..29;NEWLINE 29..30;KEYWORD_RESCUE 32..38;CONSTANT 39..40;NEWLINE 40..41;KEYWORD_ELSE 43..47;IGNORED_NEWLINE 47..48;IDENTIFIER 52..53;NEWLINE 53..54;KEYWORD_ENSURE 56..62;IGNORED_NEWLINE 62..63;IDENTIFIER 67..68;NEWLINE 68..69;KEYWORD_END 71..74;NEWLINE 74..75;KEYWORD_END 75..78;NEWLINE 78..79;"
        );
        assert!(unknown.is_empty());
    }
}
