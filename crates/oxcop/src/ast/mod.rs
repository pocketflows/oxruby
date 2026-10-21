//! Port of rubocop-ast: the `parser`-gem-shaped AST that cops work on.
//!
//! RuboCop's cops are written against the whitequark `parser` AST (node
//! types such as `send`, `lvasgn`, `dstr` and their source maps). oxcop parses
//! with Prism and translates the result into that shape, porting
//! `Prism::Translation::Parser::Compiler` and the parts of
//! `Parser::Builders::Default` it relies on (`builder.rs`, `compiler.rs`), so
//! cops, node patterns and offense locations port one-to-one.
//!
//! Nodes live in an arena ([`Ast`]) laid out in pre-order, so a node's
//! descendants are the ids right after it, up to `subtree_end`.

mod builder;
pub mod comment_associator;
mod compiler;
pub mod lexer;
pub mod node;
pub mod node_pattern;
pub mod node_type;
pub mod token;
pub mod token_builder;

use std::fmt::Write as _;

pub use node::Node;
pub use node_type::NodeType;

use crate::processed_source::Range;

pub type NodeId = u32;

/// A child of a node: another node, `nil`, or a literal value (symbol,
/// string, number), exactly as in `Parser::AST::Node#children`.
#[derive(Debug, Clone, PartialEq)]
pub enum Child {
    Node(NodeId),
    Nil,
    Sym(String),
    Str(String),
    /// A string value that is not valid UTF-8.
    Bytes(Vec<u8>),
    Int(i64),
    /// An integer that does not fit in an `i64`, in decimal.
    BigInt(String),
    Float(f64),
    /// `numerator/denominator`, in decimal.
    Rational(String, String),
    /// A complex number with a zero real part; holds the imaginary part.
    Complex(Box<Child>),
}

impl Child {
    pub fn as_node(&self) -> Option<NodeId> {
        match self {
            Child::Node(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_sym(&self) -> Option<&str> {
        match self {
            Child::Sym(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Child::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A string child from raw bytes.
    pub fn string(bytes: Vec<u8>) -> Child {
        match String::from_utf8(bytes) {
            Ok(s) => Child::Str(s),
            Err(e) => Child::Bytes(e.into_bytes()),
        }
    }

    /// The bytes of a string child.
    pub fn str_bytes(&self) -> Option<&[u8]> {
        match self {
            Child::Str(s) => Some(s.as_bytes()),
            Child::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Child::Nil)
    }
}

/// The class of a `Parser::Source::Map`; decides which location fields a
/// node has (`node.loc.respond_to?(:end)` and friends).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapKind {
    /// A node without a source map (only `Encoding` in `__ENCODING__`).
    #[default]
    None,
    Map,
    Collection,
    Condition,
    Constant,
    Definition,
    For,
    Heredoc,
    Index,
    Keyword,
    MethodDefinition,
    Operator,
    RescueBody,
    Send,
    Ternary,
    Variable,
}

impl MapKind {
    pub fn name(self) -> &'static str {
        match self {
            MapKind::None => "NilClass",
            MapKind::Map => "Map",
            MapKind::Collection => "Collection",
            MapKind::Condition => "Condition",
            MapKind::Constant => "Constant",
            MapKind::Definition => "Definition",
            MapKind::For => "For",
            MapKind::Heredoc => "Heredoc",
            MapKind::Index => "Index",
            MapKind::Keyword => "Keyword",
            MapKind::MethodDefinition => "MethodDefinition",
            MapKind::Operator => "Operator",
            MapKind::RescueBody => "RescueBody",
            MapKind::Send => "Send",
            MapKind::Ternary => "Ternary",
            MapKind::Variable => "Variable",
        }
    }
}

/// A `Parser::Source::Map`. Every map class's fields are here; which ones a
/// node really has is given by `kind`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Loc {
    pub kind: MapKind,
    pub expression: Option<Range>,
    pub keyword: Option<Range>,
    pub begin: Option<Range>,
    pub end: Option<Range>,
    pub else_: Option<Range>,
    pub selector: Option<Range>,
    pub dot: Option<Range>,
    pub operator: Option<Range>,
    pub name: Option<Range>,
    pub double_colon: Option<Range>,
    pub question: Option<Range>,
    pub colon: Option<Range>,
    pub assoc: Option<Range>,
    pub in_: Option<Range>,
    pub heredoc_body: Option<Range>,
    pub heredoc_end: Option<Range>,
    pub assignment: Option<Range>,
}

impl Loc {
    pub fn new(kind: MapKind, expression: Option<Range>) -> Self {
        Self {
            kind,
            expression,
            ..Default::default()
        }
    }

    /// The fields that are set, in alphabetical order (for `--dump-ast`).
    fn ranges(&self) -> Vec<(&'static str, Range)> {
        let fields = [
            ("assignment", self.assignment),
            ("assoc", self.assoc),
            ("begin", self.begin),
            ("colon", self.colon),
            ("dot", self.dot),
            ("double_colon", self.double_colon),
            ("else", self.else_),
            ("end", self.end),
            ("expression", self.expression),
            ("heredoc_body", self.heredoc_body),
            ("heredoc_end", self.heredoc_end),
            ("in", self.in_),
            ("keyword", self.keyword),
            ("name", self.name),
            ("operator", self.operator),
            ("question", self.question),
            ("selector", self.selector),
        ];
        fields
            .into_iter()
            .filter_map(|(name, range)| range.map(|r| (name, r)))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct NodeData {
    pub ty: NodeType,
    pub children: Vec<Child>,
    pub loc: Loc,
    pub parent: Option<NodeId>,
    /// One past the id of the last descendant (pre-order layout).
    pub subtree_end: NodeId,
}

/// The translated AST of one source file.
#[derive(Debug, Default)]
pub struct Ast {
    nodes: Vec<NodeData>,
    /// The (newline-normalized) source, for `node.source` and line numbers.
    source: String,
    line_starts: Vec<usize>,
}

impl Ast {
    /// Translates a Prism parse result. Returns an empty AST for an empty
    /// program (RuboCop's `ast` is `nil` then).
    pub fn from_prism(result: &ruby_prism::ParseResult<'_>, source: &str, file_name: &str) -> Self {
        let mut compiler = compiler::Compiler::new(source, file_name);
        let root = compiler.compile(&result.node());
        let tree = compiler.into_tree();
        let mut ast = match root {
            Some(root) => Self::layout(tree, root),
            None => Self::default(),
        };
        ast.source = source.to_owned();
        ast.line_starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        ast
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// 1-based line of a byte offset.
    pub fn line_of(&self, pos: usize) -> usize {
        match self.line_starts.binary_search(&pos) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }

    /// 0-based column (in characters) of a byte offset.
    pub fn column_of(&self, pos: usize) -> usize {
        let line = self.line_of(pos);
        let start = self.line_starts[line - 1];
        self.source[start..pos].chars().count()
    }

    /// Re-lays the nodes reachable from `root` out in pre-order.
    fn layout(tree: Vec<builder::BuildNode>, root: NodeId) -> Self {
        let mut nodes: Vec<NodeData> = Vec::with_capacity(tree.len());
        let mut tree: Vec<Option<builder::BuildNode>> = tree.into_iter().map(Some).collect();
        fn visit(
            tree: &mut Vec<Option<builder::BuildNode>>,
            nodes: &mut Vec<NodeData>,
            old: NodeId,
            parent: Option<NodeId>,
        ) -> NodeId {
            let built = tree[old as usize]
                .take()
                .expect("a node is reachable from only one parent");
            let id = nodes.len() as NodeId;
            nodes.push(NodeData {
                ty: built.ty,
                children: Vec::new(),
                loc: built.loc,
                parent,
                subtree_end: 0,
            });
            let children = built
                .children
                .into_iter()
                .map(|child| match child {
                    Child::Node(old_child) => Child::Node(visit(tree, nodes, old_child, Some(id))),
                    other => other,
                })
                .collect();
            nodes[id as usize].children = children;
            nodes[id as usize].subtree_end = nodes.len() as NodeId;
            id
        }
        visit(&mut tree, &mut nodes, root, None);
        Self {
            nodes,
            ..Default::default()
        }
    }

    pub fn root(&self) -> Option<Node<'_>> {
        (!self.nodes.is_empty()).then(|| Node::new(self, 0))
    }

    pub fn node(&self, id: NodeId) -> Node<'_> {
        Node::new(self, id)
    }

    pub fn data(&self, id: NodeId) -> &NodeData {
        &self.nodes[id as usize]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// All nodes in pre-order.
    pub fn nodes(&self) -> impl Iterator<Item = Node<'_>> {
        (0..self.nodes.len() as NodeId).map(|id| Node::new(self, id))
    }

    /// The format of `script/dump_ast`, for differential testing of the
    /// translation against RuboCop.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        match self.root() {
            Some(root) => self.dump_node(root.id(), 0, &mut out),
            None => out.push_str("nil\n"),
        }
        out
    }

    fn dump_node(&self, id: NodeId, depth: usize, out: &mut String) {
        let data = self.data(id);
        let indent = "  ".repeat(depth);
        let ranges: Vec<String> = data
            .loc
            .ranges()
            .into_iter()
            .map(|(name, r)| format!("{name}={}..{}", r.begin_pos, r.end_pos))
            .collect();
        let _ = writeln!(
            out,
            "{indent}{} {}{{{}}}",
            data.ty.name(),
            data.loc.kind.name(),
            ranges.join(",")
        );
        for child in &data.children {
            match child {
                Child::Node(child) => self.dump_node(*child, depth + 1, out),
                other => {
                    let _ = writeln!(out, "{indent}  {}", dump_value(other));
                }
            }
        }
    }
}

fn escape(s: &str) -> String {
    escape_bytes(s.as_bytes())
}

fn escape_bytes(s: &[u8]) -> String {
    let mut out = String::with_capacity(s.len());
    for &byte in s {
        if (0x20..=0x7e).contains(&byte) && byte != b'"' && byte != b'\\' {
            out.push(byte as char);
        } else {
            let _ = write!(out, "\\x{byte:02X}");
        }
    }
    out
}

fn dump_value(child: &Child) -> String {
    match child {
        Child::Node(_) => unreachable!(),
        Child::Nil => "nil".to_owned(),
        Child::Sym(s) => format!(":{}", escape(s)),
        Child::Str(s) => format!("\"{}\"", escape(s)),
        Child::Bytes(b) => format!("\"{}\"", escape_bytes(b)),
        Child::Int(i) => i.to_string(),
        Child::BigInt(s) => s.clone(),
        Child::Float(f) => format!("f{:016x}", f.to_bits()),
        Child::Rational(n, d) => format!("r{n}/{d}"),
        Child::Complex(imaginary) => format!("c0,{}", dump_value(imaginary)),
    }
}
