//! Port of syntax_tree's node classes (`lib/syntax_tree/node.rb`): the
//! fields, locations and `child_nodes` of every node. The `format` methods
//! live in `format.rs`.
//!
//! Nodes are stored in an arena and referred to by `NodeId`, which gives the
//! identity comparisons the formatter relies on (`body.first == node`).
//! Locations use byte offsets for `start_char`/`end_char`; only their order
//! and the source slices they delimit matter.

use std::fmt::Write as _;

pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Location {
    pub start_line: usize,
    pub start_char: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_char: usize,
    pub end_column: usize,
}

impl Location {
    /// `Location#to`
    pub fn to(&self, other: &Location) -> Location {
        Location {
            start_line: self.start_line,
            start_char: self.start_char,
            start_column: self.start_column,
            end_line: self.end_line.max(other.end_line),
            end_char: other.end_char,
            end_column: other.end_column,
        }
    }

    /// `Location.fixed`
    pub fn fixed(line: usize, char: usize, column: usize) -> Location {
        Location {
            start_line: line,
            start_char: char,
            start_column: column,
            end_line: line,
            end_char: char,
            end_column: column,
        }
    }
}

/// The operator of a `CallNode`, `CommandCall`, `Field` or `DefNode`:
/// `nil`, the `:"::"` symbol, or an `Op`/`Period` token node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOperator {
    Nil,
    Colon2,
    Node(NodeId),
}

/// `nil | :nil | Node` (`VarField#value`, `Params#keyword_rest`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NilOr {
    None,
    NilSymbol,
    Node(NodeId),
}

impl NilOr {
    pub fn node(self) -> Option<NodeId> {
        match self {
            NilOr::Node(id) => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Kind {
    BEGINBlock {
        lbrace: NodeId,
        statements: NodeId,
    },
    CHAR {
        value: String,
    },
    ENDBlock {
        lbrace: NodeId,
        statements: NodeId,
    },
    EndContent {
        value: String,
    },
    AliasNode {
        left: NodeId,
        right: NodeId,
    },
    ARef {
        collection: NodeId,
        index: Option<NodeId>,
    },
    ARefField {
        collection: NodeId,
        index: Option<NodeId>,
    },
    ArgParen {
        arguments: Option<NodeId>,
    },
    Args {
        parts: Vec<NodeId>,
    },
    ArgBlock {
        value: Option<NodeId>,
    },
    ArgStar {
        value: Option<NodeId>,
    },
    ArgsForward,
    ArrayLiteral {
        lbracket: Option<NodeId>,
        contents: Option<NodeId>,
    },
    AryPtn {
        constant: Option<NodeId>,
        requireds: Vec<NodeId>,
        rest: Option<NodeId>,
        posts: Vec<NodeId>,
    },
    Assign {
        target: NodeId,
        value: NodeId,
    },
    Assoc {
        key: NodeId,
        value: Option<NodeId>,
    },
    AssocSplat {
        value: Option<NodeId>,
    },
    Backref {
        value: String,
    },
    Backtick {
        value: String,
    },
    BareAssocHash {
        assocs: Vec<NodeId>,
    },
    Begin {
        bodystmt: NodeId,
    },
    PinnedBegin {
        statement: NodeId,
    },
    Binary {
        left: NodeId,
        operator: String,
        right: NodeId,
    },
    BlockVar {
        params: NodeId,
        locals: Vec<NodeId>,
    },
    BlockArg {
        name: Option<NodeId>,
    },
    BodyStmt {
        statements: NodeId,
        rescue_clause: Option<NodeId>,
        else_keyword: Option<NodeId>,
        else_clause: Option<NodeId>,
        ensure_clause: Option<NodeId>,
    },
    Break {
        arguments: NodeId,
    },
    CallNode {
        receiver: Option<NodeId>,
        operator: CallOperator,
        /// `None` is the `:call` message (`foo.()`).
        message: Option<NodeId>,
        arguments: Option<NodeId>,
    },
    Case {
        keyword: NodeId,
        value: Option<NodeId>,
        consequent: NodeId,
    },
    RAssign {
        value: NodeId,
        operator: NodeId,
        pattern: NodeId,
    },
    ClassDeclaration {
        constant: NodeId,
        superclass: Option<NodeId>,
        bodystmt: NodeId,
    },
    Comma {
        value: String,
    },
    Command {
        message: NodeId,
        arguments: NodeId,
        block: Option<NodeId>,
    },
    CommandCall {
        receiver: NodeId,
        operator: CallOperator,
        message: Option<NodeId>,
        arguments: Option<NodeId>,
        block: Option<NodeId>,
    },
    Comment {
        value: String,
        inline: bool,
        leading: bool,
        trailing: bool,
    },
    Const {
        value: String,
    },
    ConstPathField {
        parent: NodeId,
        constant: NodeId,
    },
    ConstPathRef {
        parent: NodeId,
        constant: NodeId,
    },
    ConstRef {
        constant: NodeId,
    },
    CVar {
        value: String,
    },
    DefNode {
        target: Option<NodeId>,
        operator: CallOperator,
        name: NodeId,
        params: Option<NodeId>,
        bodystmt: NodeId,
    },
    Defined {
        value: NodeId,
    },
    BlockNode {
        opening: NodeId,
        block_var: Option<NodeId>,
        bodystmt: NodeId,
    },
    RangeNode {
        left: Option<NodeId>,
        operator: NodeId,
        right: Option<NodeId>,
    },
    DynaSymbol {
        parts: Vec<NodeId>,
        quote: Option<String>,
    },
    Else {
        keyword: NodeId,
        statements: NodeId,
    },
    Elsif {
        predicate: NodeId,
        statements: NodeId,
        consequent: Option<NodeId>,
    },
    EmbDoc {
        value: String,
        leading: bool,
        trailing: bool,
    },
    Ensure {
        keyword: NodeId,
        statements: NodeId,
    },
    ExcessedComma {
        value: String,
    },
    Field {
        parent: NodeId,
        operator: CallOperator,
        name: NodeId,
    },
    FloatLiteral {
        value: String,
    },
    FndPtn {
        constant: Option<NodeId>,
        left: NodeId,
        values: Vec<NodeId>,
        right: NodeId,
    },
    For {
        index: NodeId,
        collection: NodeId,
        statements: NodeId,
    },
    GVar {
        value: String,
    },
    HashLiteral {
        lbrace: NodeId,
        assocs: Vec<NodeId>,
    },
    Heredoc {
        beginning: NodeId,
        ending: Option<NodeId>,
        dedent: usize,
        parts: Vec<NodeId>,
    },
    HeredocBeg {
        value: String,
    },
    HeredocEnd {
        value: String,
    },
    HshPtn {
        constant: Option<NodeId>,
        keywords: Vec<(NodeId, Option<NodeId>)>,
        keyword_rest: Option<NodeId>,
    },
    Ident {
        value: String,
    },
    IfNode {
        predicate: NodeId,
        statements: NodeId,
        consequent: Option<NodeId>,
    },
    IfOp {
        predicate: NodeId,
        truthy: NodeId,
        falsy: NodeId,
    },
    Imaginary {
        value: String,
    },
    In {
        pattern: NodeId,
        statements: NodeId,
        consequent: Option<NodeId>,
    },
    Int {
        value: String,
    },
    IVar {
        value: String,
    },
    Kw {
        value: String,
    },
    KwRestParam {
        name: Option<NodeId>,
    },
    Label {
        value: String,
    },
    Lambda {
        params: NodeId,
        statements: NodeId,
    },
    LambdaVar {
        params: NodeId,
        locals: Vec<NodeId>,
    },
    LBrace {
        value: String,
    },
    LBracket {
        value: String,
    },
    LParen {
        value: String,
    },
    MAssign {
        target: NodeId,
        value: NodeId,
    },
    MethodAddBlock {
        call: NodeId,
        block: NodeId,
    },
    MLHS {
        parts: Vec<NodeId>,
        comma: bool,
    },
    MLHSParen {
        contents: NodeId,
        comma: bool,
    },
    ModuleDeclaration {
        constant: NodeId,
        bodystmt: NodeId,
    },
    MRHS {
        parts: Vec<NodeId>,
    },
    Next {
        arguments: NodeId,
    },
    Op {
        value: String,
    },
    OpAssign {
        target: NodeId,
        operator: NodeId,
        value: NodeId,
    },
    Params {
        requireds: Vec<NodeId>,
        optionals: Vec<(NodeId, NodeId)>,
        rest: Option<NodeId>,
        posts: Vec<NodeId>,
        keywords: Vec<(NodeId, Option<NodeId>)>,
        keyword_rest: NilOr,
        block: Option<NodeId>,
    },
    Paren {
        lparen: NodeId,
        contents: Option<NodeId>,
    },
    Period {
        value: String,
    },
    Program {
        statements: NodeId,
    },
    QSymbols {
        beginning: NodeId,
        elements: Vec<NodeId>,
    },
    QSymbolsBeg {
        value: String,
    },
    QWords {
        beginning: NodeId,
        elements: Vec<NodeId>,
    },
    QWordsBeg {
        value: String,
    },
    RationalLiteral {
        value: String,
    },
    Redo,
    RegexpLiteral {
        beginning: String,
        ending: String,
        parts: Vec<NodeId>,
    },
    RescueEx {
        exceptions: Option<NodeId>,
        variable: Option<NodeId>,
    },
    Rescue {
        keyword: NodeId,
        exception: Option<NodeId>,
        statements: NodeId,
        consequent: Option<NodeId>,
    },
    RescueMod {
        statement: NodeId,
        value: NodeId,
    },
    RestParam {
        name: Option<NodeId>,
    },
    Retry,
    ReturnNode {
        arguments: Option<NodeId>,
    },
    SClass {
        target: NodeId,
        bodystmt: NodeId,
    },
    Statements {
        body: Vec<NodeId>,
    },
    StringConcat {
        left: NodeId,
        right: NodeId,
    },
    StringDVar {
        variable: NodeId,
    },
    StringEmbExpr {
        statements: NodeId,
    },
    StringLiteral {
        parts: Vec<NodeId>,
        quote: Option<String>,
    },
    Super {
        arguments: NodeId,
    },
    SymbolLiteral {
        value: NodeId,
    },
    Symbols {
        beginning: NodeId,
        elements: Vec<NodeId>,
    },
    SymbolsBeg {
        value: String,
    },
    TopConstField {
        constant: NodeId,
    },
    TopConstRef {
        constant: NodeId,
    },
    TStringContent {
        value: String,
    },
    Not {
        statement: Option<NodeId>,
        parentheses: bool,
    },
    Unary {
        operator: String,
        statement: NodeId,
    },
    Undef {
        symbols: Vec<NodeId>,
    },
    UnlessNode {
        predicate: NodeId,
        statements: NodeId,
        consequent: Option<NodeId>,
    },
    UntilNode {
        predicate: NodeId,
        statements: NodeId,
    },
    VarField {
        value: NilOr,
    },
    VarRef {
        value: NodeId,
    },
    PinnedVarRef {
        value: NodeId,
    },
    VCall {
        value: NodeId,
    },
    VoidStmt,
    When {
        arguments: NodeId,
        statements: NodeId,
        consequent: Option<NodeId>,
    },
    WhileNode {
        predicate: NodeId,
        statements: NodeId,
    },
    Word {
        parts: Vec<NodeId>,
    },
    Words {
        beginning: NodeId,
        elements: Vec<NodeId>,
    },
    WordsBeg {
        value: String,
    },
    XStringLiteral {
        parts: Vec<NodeId>,
    },
    YieldNode {
        arguments: Option<NodeId>,
    },
    ZSuper,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub location: Location,
    pub comments: Vec<NodeId>,
}

/// The tree: an arena of nodes plus the source it was parsed from.
#[derive(Debug)]
pub struct Ast {
    pub nodes: Vec<Node>,
    pub source: String,
    pub root: NodeId,
}

impl Ast {
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn kind(&self, id: NodeId) -> &Kind {
        &self.nodes[id as usize].kind
    }

    pub fn loc(&self, id: NodeId) -> &Location {
        &self.nodes[id as usize].location
    }

    pub fn comments(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id as usize].comments
    }

    /// The `value` of token-like nodes (`Ident`, `Kw`, `Op`, ...).
    pub fn value(&self, id: NodeId) -> &str {
        match self.kind(id) {
            Kind::CHAR { value }
            | Kind::EndContent { value }
            | Kind::Backref { value }
            | Kind::Backtick { value }
            | Kind::Comma { value }
            | Kind::Comment { value, .. }
            | Kind::Const { value }
            | Kind::CVar { value }
            | Kind::EmbDoc { value, .. }
            | Kind::ExcessedComma { value }
            | Kind::FloatLiteral { value }
            | Kind::GVar { value }
            | Kind::HeredocBeg { value }
            | Kind::HeredocEnd { value }
            | Kind::Ident { value }
            | Kind::Imaginary { value }
            | Kind::Int { value }
            | Kind::IVar { value }
            | Kind::Kw { value }
            | Kind::Label { value }
            | Kind::LBrace { value }
            | Kind::LBracket { value }
            | Kind::LParen { value }
            | Kind::Op { value }
            | Kind::Period { value }
            | Kind::QSymbolsBeg { value }
            | Kind::QWordsBeg { value }
            | Kind::RationalLiteral { value }
            | Kind::SymbolsBeg { value }
            | Kind::TStringContent { value }
            | Kind::WordsBeg { value } => value,
            _ => "",
        }
    }

    pub fn class_name(&self, id: NodeId) -> &'static str {
        kind_name(self.kind(id))
    }

    /// `Node#child_nodes`, in syntax_tree's order, with `nil`s kept.
    pub fn child_nodes(&self, id: NodeId) -> Vec<Option<NodeId>> {
        child_nodes(self.kind(id))
    }

    // ------------------------------------------------------------------
    // Debug dump, in the format of tools/dump_ast.rb.
    // ------------------------------------------------------------------

    pub fn dump(&self) -> String {
        let mut char_index = vec![0usize; self.source.len() + 1];
        let mut count = 0;
        for (byte, ch) in self.source.char_indices() {
            for slot in char_index.iter_mut().skip(byte).take(ch.len_utf8()) {
                *slot = count;
            }
            count += 1;
        }
        char_index[self.source.len()] = count;
        let mut out = String::new();
        self.dump_node(self.root, 0, &mut out, "", &char_index);
        out
    }

    fn dump_loc(&self, id: NodeId, chars: &[usize]) -> String {
        let l = self.loc(id);
        let c = |b: usize| chars.get(b).copied().unwrap_or(b);
        format!(
            "{}:{}-{}:{}",
            l.start_line,
            c(l.start_char),
            l.end_line,
            c(l.end_char)
        )
    }

    fn dump_node(&self, id: NodeId, indent: usize, out: &mut String, label: &str, chars: &[usize]) {
        let pad = "  ".repeat(indent);
        match self.kind(id) {
            Kind::Comment {
                value,
                leading,
                trailing,
                ..
            }
            | Kind::EmbDoc {
                value,
                leading,
                trailing,
            } => {
                let flag = if *leading {
                    "L"
                } else if *trailing {
                    "T"
                } else {
                    "-"
                };
                let _ = writeln!(
                    out,
                    "{pad}{label}{} {} {flag} {}",
                    self.class_name(id),
                    self.dump_loc(id, chars),
                    ruby_inspect(value)
                );
                return;
            }
            _ => {}
        }
        let _ = writeln!(
            out,
            "{pad}{label}{} {}",
            self.class_name(id),
            self.dump_loc(id, chars)
        );
        for &comment in self.comments(id) {
            self.dump_node(comment, indent + 1, out, "#", chars);
        }
        let mut fields = Vec::new();
        self.fields(id, &mut fields);
        for (name, field) in fields {
            let label = format!(".{name} ");
            self.dump_field(&field, indent + 1, out, &label, chars);
        }
    }

    fn dump_field(
        &self,
        field: &Field,
        indent: usize,
        out: &mut String,
        label: &str,
        chars: &[usize],
    ) {
        let pad = "  ".repeat(indent);
        match field {
            Field::Node(id) => self.dump_node(*id, indent, out, label, chars),
            Field::Nil => {
                let _ = writeln!(out, "{pad}{label}nil");
            }
            Field::Str(s) => {
                let _ = writeln!(out, "{pad}{label}{}", ruby_inspect(s));
            }
            Field::Raw(s) => {
                let _ = writeln!(out, "{pad}{label}{s}");
            }
            Field::List(items) => {
                let _ = writeln!(out, "{pad}{label}[{}]", items.len());
                for item in items {
                    self.dump_field(item, indent + 1, out, "", chars);
                }
            }
        }
    }

    fn fields(&self, id: NodeId, f: &mut Vec<(&'static str, Field)>) {
        use Field as F;
        let n = |id: NodeId| F::Node(id);
        let o = |id: Option<NodeId>| id.map_or(F::Nil, F::Node);
        let l = |ids: &[NodeId]| F::List(ids.iter().map(|&id| F::Node(id)).collect());
        let s = |v: &str| F::Str(v.to_string());
        let op = |o: &CallOperator| match o {
            CallOperator::Nil => F::Nil,
            CallOperator::Colon2 => F::Raw(":\"::\"".to_string()),
            CallOperator::Node(id) => F::Node(*id),
        };
        let nil_or = |v: &NilOr| match v {
            NilOr::None => F::Nil,
            NilOr::NilSymbol => F::Raw(":nil".to_string()),
            NilOr::Node(id) => F::Node(*id),
        };
        match self.kind(id) {
            Kind::BEGINBlock { lbrace, statements } | Kind::ENDBlock { lbrace, statements } => {
                f.push(("lbrace", n(*lbrace)));
                f.push(("statements", n(*statements)));
            }
            Kind::CHAR { value }
            | Kind::EndContent { value }
            | Kind::Backref { value }
            | Kind::Backtick { value }
            | Kind::Comma { value }
            | Kind::Const { value }
            | Kind::CVar { value }
            | Kind::ExcessedComma { value }
            | Kind::FloatLiteral { value }
            | Kind::GVar { value }
            | Kind::HeredocBeg { value }
            | Kind::HeredocEnd { value }
            | Kind::Ident { value }
            | Kind::Imaginary { value }
            | Kind::Int { value }
            | Kind::IVar { value }
            | Kind::Kw { value }
            | Kind::Label { value }
            | Kind::LBrace { value }
            | Kind::LBracket { value }
            | Kind::LParen { value }
            | Kind::Op { value }
            | Kind::Period { value }
            | Kind::QSymbolsBeg { value }
            | Kind::QWordsBeg { value }
            | Kind::RationalLiteral { value }
            | Kind::SymbolsBeg { value }
            | Kind::TStringContent { value }
            | Kind::WordsBeg { value } => f.push(("value", s(value))),
            Kind::AliasNode { left, right } => {
                f.push(("left", n(*left)));
                f.push(("right", n(*right)));
            }
            Kind::ARef { collection, index } | Kind::ARefField { collection, index } => {
                f.push(("collection", n(*collection)));
                f.push(("index", o(*index)));
            }
            Kind::ArgParen { arguments } => f.push(("arguments", o(*arguments))),
            Kind::Args { parts } => f.push(("parts", l(parts))),
            Kind::ArgBlock { value } | Kind::ArgStar { value } | Kind::AssocSplat { value } => {
                f.push(("value", o(*value)))
            }
            Kind::ArgsForward | Kind::Redo | Kind::Retry | Kind::VoidStmt | Kind::ZSuper => {}
            Kind::ArrayLiteral { lbracket, contents } => {
                f.push(("lbracket", o(*lbracket)));
                f.push(("contents", o(*contents)));
            }
            Kind::AryPtn {
                constant,
                requireds,
                rest,
                posts,
            } => {
                f.push(("constant", o(*constant)));
                f.push(("requireds", l(requireds)));
                f.push(("rest", o(*rest)));
                f.push(("posts", l(posts)));
            }
            Kind::Assign { target, value } | Kind::MAssign { target, value } => {
                f.push(("target", n(*target)));
                f.push(("value", n(*value)));
            }
            Kind::Assoc { key, value } => {
                f.push(("key", n(*key)));
                f.push(("value", o(*value)));
            }
            Kind::BareAssocHash { assocs } => f.push(("assocs", l(assocs))),
            Kind::Begin { bodystmt } => f.push(("bodystmt", n(*bodystmt))),
            Kind::PinnedBegin { statement } => f.push(("statement", n(*statement))),
            Kind::Binary {
                left,
                operator,
                right,
            } => {
                f.push(("left", n(*left)));
                f.push(("operator", F::Raw(ruby_symbol(operator))));
                f.push(("right", n(*right)));
            }
            Kind::BlockVar { params, locals } | Kind::LambdaVar { params, locals } => {
                f.push(("params", n(*params)));
                f.push(("locals", l(locals)));
            }
            Kind::BlockArg { name } | Kind::KwRestParam { name } | Kind::RestParam { name } => {
                f.push(("name", o(*name)))
            }
            Kind::BodyStmt {
                statements,
                rescue_clause,
                else_keyword,
                else_clause,
                ensure_clause,
            } => {
                f.push(("statements", n(*statements)));
                f.push(("rescue_clause", o(*rescue_clause)));
                f.push(("else_keyword", o(*else_keyword)));
                f.push(("else_clause", o(*else_clause)));
                f.push(("ensure_clause", o(*ensure_clause)));
            }
            Kind::Break { arguments } | Kind::Next { arguments } => {
                f.push(("arguments", n(*arguments)))
            }
            Kind::CallNode {
                receiver,
                operator,
                message,
                arguments,
            } => {
                f.push(("receiver", o(*receiver)));
                f.push(("operator", op(operator)));
                f.push((
                    "message",
                    message.map_or(F::Raw(":call".to_string()), F::Node),
                ));
                f.push(("arguments", o(*arguments)));
            }
            Kind::Case {
                keyword,
                value,
                consequent,
            } => {
                f.push(("keyword", n(*keyword)));
                f.push(("value", o(*value)));
                f.push(("consequent", n(*consequent)));
            }
            Kind::RAssign {
                value,
                operator,
                pattern,
            } => {
                f.push(("value", n(*value)));
                f.push(("operator", n(*operator)));
                f.push(("pattern", n(*pattern)));
            }
            Kind::ClassDeclaration {
                constant,
                superclass,
                bodystmt,
            } => {
                f.push(("constant", n(*constant)));
                f.push(("superclass", o(*superclass)));
                f.push(("bodystmt", n(*bodystmt)));
            }
            Kind::Command {
                message,
                arguments,
                block,
            } => {
                f.push(("message", n(*message)));
                f.push(("arguments", n(*arguments)));
                f.push(("block", o(*block)));
            }
            Kind::CommandCall {
                receiver,
                operator,
                message,
                arguments,
                block,
            } => {
                f.push(("receiver", n(*receiver)));
                f.push(("operator", op(operator)));
                f.push((
                    "message",
                    message.map_or(F::Raw(":call".to_string()), F::Node),
                ));
                f.push(("arguments", o(*arguments)));
                f.push(("block", o(*block)));
            }
            Kind::Comment { .. } | Kind::EmbDoc { .. } => {}
            Kind::ConstPathField { parent, constant } | Kind::ConstPathRef { parent, constant } => {
                f.push(("parent", n(*parent)));
                f.push(("constant", n(*constant)));
            }
            Kind::ConstRef { constant }
            | Kind::TopConstField { constant }
            | Kind::TopConstRef { constant } => f.push(("constant", n(*constant))),
            Kind::DefNode {
                target,
                operator,
                name,
                params,
                bodystmt,
            } => {
                f.push(("target", o(*target)));
                f.push(("operator", op(operator)));
                f.push(("name", n(*name)));
                f.push(("params", o(*params)));
                f.push(("bodystmt", n(*bodystmt)));
            }
            Kind::Defined { value } => f.push(("value", n(*value))),
            Kind::BlockNode {
                opening,
                block_var,
                bodystmt,
            } => {
                f.push(("opening", n(*opening)));
                f.push(("block_var", o(*block_var)));
                f.push(("bodystmt", n(*bodystmt)));
            }
            Kind::RangeNode {
                left,
                operator,
                right,
            } => {
                f.push(("left", o(*left)));
                f.push(("operator", n(*operator)));
                f.push(("right", o(*right)));
            }
            Kind::DynaSymbol { parts, quote } => {
                f.push(("parts", l(parts)));
                f.push(("quote", quote.as_deref().map_or(F::Nil, s)));
            }
            Kind::Else {
                keyword,
                statements,
            }
            | Kind::Ensure {
                keyword,
                statements,
            } => {
                f.push(("keyword", n(*keyword)));
                f.push(("statements", n(*statements)));
            }
            Kind::Elsif {
                predicate,
                statements,
                consequent,
            }
            | Kind::IfNode {
                predicate,
                statements,
                consequent,
            }
            | Kind::UnlessNode {
                predicate,
                statements,
                consequent,
            } => {
                f.push(("predicate", n(*predicate)));
                f.push(("statements", n(*statements)));
                f.push(("consequent", o(*consequent)));
            }
            Kind::Field {
                parent,
                operator,
                name,
            } => {
                f.push(("parent", n(*parent)));
                f.push(("operator", op(operator)));
                f.push(("name", n(*name)));
            }
            Kind::FndPtn {
                constant,
                left,
                values,
                right,
            } => {
                f.push(("constant", o(*constant)));
                f.push(("left", n(*left)));
                f.push(("values", l(values)));
                f.push(("right", n(*right)));
            }
            Kind::For {
                index,
                collection,
                statements,
            } => {
                f.push(("index", n(*index)));
                f.push(("collection", n(*collection)));
                f.push(("statements", n(*statements)));
            }
            Kind::HashLiteral { lbrace, assocs } => {
                f.push(("lbrace", n(*lbrace)));
                f.push(("assocs", l(assocs)));
            }
            Kind::Heredoc {
                beginning,
                ending,
                dedent,
                parts,
            } => {
                f.push(("beginning", n(*beginning)));
                f.push(("ending", o(*ending)));
                f.push(("dedent", F::Raw(dedent.to_string())));
                f.push(("parts", l(parts)));
            }
            Kind::HshPtn {
                constant,
                keywords,
                keyword_rest,
            } => {
                f.push(("constant", o(*constant)));
                f.push((
                    "keywords",
                    F::List(
                        keywords
                            .iter()
                            .map(|(k, v)| F::List(vec![n(*k), o(*v)]))
                            .collect(),
                    ),
                ));
                f.push(("keyword_rest", o(*keyword_rest)));
            }
            Kind::IfOp {
                predicate,
                truthy,
                falsy,
            } => {
                f.push(("predicate", n(*predicate)));
                f.push(("truthy", n(*truthy)));
                f.push(("falsy", n(*falsy)));
            }
            Kind::In {
                pattern,
                statements,
                consequent,
            } => {
                f.push(("pattern", n(*pattern)));
                f.push(("statements", n(*statements)));
                f.push(("consequent", o(*consequent)));
            }
            Kind::Lambda { params, statements } => {
                f.push(("params", n(*params)));
                f.push(("statements", n(*statements)));
            }
            Kind::MethodAddBlock { call, block } => {
                f.push(("call", n(*call)));
                f.push(("block", n(*block)));
            }
            Kind::MLHS { parts, comma } => {
                f.push(("parts", l(parts)));
                f.push(("comma", F::Raw(comma.to_string())));
            }
            Kind::MLHSParen { contents, comma } => {
                f.push(("contents", n(*contents)));
                f.push(("comma", F::Raw(comma.to_string())));
            }
            Kind::ModuleDeclaration { constant, bodystmt } => {
                f.push(("constant", n(*constant)));
                f.push(("bodystmt", n(*bodystmt)));
            }
            Kind::MRHS { parts } => f.push(("parts", l(parts))),
            Kind::OpAssign {
                target,
                operator,
                value,
            } => {
                f.push(("target", n(*target)));
                f.push(("operator", n(*operator)));
                f.push(("value", n(*value)));
            }
            Kind::Params {
                requireds,
                optionals,
                rest,
                posts,
                keywords,
                keyword_rest,
                block,
            } => {
                f.push(("requireds", l(requireds)));
                f.push((
                    "optionals",
                    F::List(
                        optionals
                            .iter()
                            .map(|(k, v)| F::List(vec![n(*k), n(*v)]))
                            .collect(),
                    ),
                ));
                f.push(("rest", o(*rest)));
                f.push(("posts", l(posts)));
                f.push((
                    "keywords",
                    F::List(
                        keywords
                            .iter()
                            .map(|(k, v)| F::List(vec![n(*k), o(*v)]))
                            .collect(),
                    ),
                ));
                f.push(("keyword_rest", nil_or(keyword_rest)));
                f.push(("block", o(*block)));
            }
            Kind::Paren { lparen, contents } => {
                f.push(("lparen", n(*lparen)));
                f.push(("contents", o(*contents)));
            }
            Kind::Program { statements } => f.push(("statements", n(*statements))),
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
                f.push(("beginning", n(*beginning)));
                f.push(("elements", l(elements)));
            }
            Kind::RegexpLiteral {
                beginning,
                ending,
                parts,
            } => {
                f.push(("beginning", s(beginning)));
                f.push(("ending", s(ending)));
                f.push(("parts", l(parts)));
            }
            Kind::RescueEx {
                exceptions,
                variable,
            } => {
                f.push(("exceptions", o(*exceptions)));
                f.push(("variable", o(*variable)));
            }
            Kind::Rescue {
                keyword,
                exception,
                statements,
                consequent,
            } => {
                f.push(("keyword", n(*keyword)));
                f.push(("exception", o(*exception)));
                f.push(("statements", n(*statements)));
                f.push(("consequent", o(*consequent)));
            }
            Kind::RescueMod { statement, value } => {
                f.push(("statement", n(*statement)));
                f.push(("value", n(*value)));
            }
            Kind::ReturnNode { arguments } | Kind::YieldNode { arguments } => {
                f.push(("arguments", o(*arguments)))
            }
            Kind::SClass { target, bodystmt } => {
                f.push(("target", n(*target)));
                f.push(("bodystmt", n(*bodystmt)));
            }
            Kind::Statements { body } => f.push(("body", l(body))),
            Kind::StringConcat { left, right } => {
                f.push(("left", n(*left)));
                f.push(("right", n(*right)));
            }
            Kind::StringDVar { variable } => f.push(("variable", n(*variable))),
            Kind::StringEmbExpr { statements } => f.push(("statements", n(*statements))),
            Kind::StringLiteral { parts, quote } => {
                f.push(("parts", l(parts)));
                f.push(("quote", quote.as_deref().map_or(F::Nil, s)));
            }
            Kind::Super { arguments } => f.push(("arguments", n(*arguments))),
            Kind::SymbolLiteral { value } => f.push(("value", n(*value))),
            Kind::Not {
                statement,
                parentheses,
            } => {
                f.push(("statement", o(*statement)));
                f.push(("parentheses", F::Raw(parentheses.to_string())));
            }
            Kind::Unary {
                operator,
                statement,
            } => {
                f.push(("operator", s(operator)));
                f.push(("statement", n(*statement)));
            }
            Kind::Undef { symbols } => f.push(("symbols", l(symbols))),
            Kind::UntilNode {
                predicate,
                statements,
            }
            | Kind::WhileNode {
                predicate,
                statements,
            } => {
                f.push(("predicate", n(*predicate)));
                f.push(("statements", n(*statements)));
            }
            Kind::VarField { value } => f.push(("value", nil_or(value))),
            Kind::VarRef { value } | Kind::PinnedVarRef { value } | Kind::VCall { value } => {
                f.push(("value", n(*value)))
            }
            Kind::When {
                arguments,
                statements,
                consequent,
            } => {
                f.push(("arguments", n(*arguments)));
                f.push(("statements", n(*statements)));
                f.push(("consequent", o(*consequent)));
            }
            Kind::Word { parts } | Kind::XStringLiteral { parts } => f.push(("parts", l(parts))),
        }
    }
}

/// `Node#child_nodes`, in syntax_tree's order, with `nil`s kept.
pub fn child_nodes(kind: &Kind) -> Vec<Option<NodeId>> {
    fn all(v: &[NodeId]) -> impl Iterator<Item = Option<NodeId>> + '_ {
        v.iter().map(|&id| Some(id))
    }
    let op = |o: &CallOperator| match o {
        CallOperator::Node(id) => Some(Some(*id)),
        CallOperator::Nil => Some(None),
        CallOperator::Colon2 => None,
    };
    match kind {
        Kind::BEGINBlock { lbrace, statements } | Kind::ENDBlock { lbrace, statements } => {
            vec![Some(*lbrace), Some(*statements)]
        }
        Kind::AliasNode { left, right } => vec![Some(*left), Some(*right)],
        Kind::ARef { collection, index } | Kind::ARefField { collection, index } => {
            vec![Some(*collection), *index]
        }
        Kind::ArgParen { arguments } => vec![*arguments],
        Kind::Args { parts } => all(parts).collect(),
        Kind::ArgBlock { value } | Kind::ArgStar { value } | Kind::AssocSplat { value } => {
            vec![*value]
        }
        Kind::ArrayLiteral { lbracket, contents } => vec![*lbracket, *contents],
        Kind::AryPtn {
            constant,
            requireds,
            rest,
            posts,
        } => {
            let mut v = vec![*constant];
            v.extend(all(requireds));
            v.push(*rest);
            v.extend(all(posts));
            v
        }
        Kind::Assign { target, value } | Kind::MAssign { target, value } => {
            vec![Some(*target), Some(*value)]
        }
        Kind::Assoc { key, value } => vec![Some(*key), *value],
        Kind::BareAssocHash { assocs } => all(assocs).collect(),
        Kind::Begin { bodystmt } => vec![Some(*bodystmt)],
        Kind::PinnedBegin { statement } => vec![Some(*statement)],
        Kind::Binary { left, right, .. } => vec![Some(*left), Some(*right)],
        Kind::BlockVar { params, locals } | Kind::LambdaVar { params, locals } => {
            let mut v = vec![Some(*params)];
            v.extend(all(locals));
            v
        }
        Kind::BlockArg { name } | Kind::KwRestParam { name } | Kind::RestParam { name } => {
            vec![*name]
        }
        Kind::BodyStmt {
            statements,
            rescue_clause,
            else_keyword,
            else_clause,
            ensure_clause,
        } => vec![
            Some(*statements),
            *rescue_clause,
            *else_keyword,
            *else_clause,
            *ensure_clause,
        ],
        Kind::Break { arguments } | Kind::Next { arguments } => vec![Some(*arguments)],
        Kind::CallNode {
            receiver,
            operator,
            message,
            arguments,
        } => {
            let mut v = vec![*receiver];
            if let Some(o) = op(operator) {
                v.push(o);
            }
            if let Some(m) = message {
                v.push(Some(*m));
            }
            v.push(*arguments);
            v
        }
        Kind::Case {
            keyword,
            value,
            consequent,
        } => vec![Some(*keyword), *value, Some(*consequent)],
        Kind::RAssign {
            value,
            operator,
            pattern,
        } => vec![Some(*value), Some(*operator), Some(*pattern)],
        Kind::ClassDeclaration {
            constant,
            superclass,
            bodystmt,
        } => vec![Some(*constant), *superclass, Some(*bodystmt)],
        Kind::Command {
            message,
            arguments,
            block,
        } => vec![Some(*message), Some(*arguments), *block],
        Kind::CommandCall {
            receiver,
            message,
            arguments,
            block,
            ..
        } => vec![Some(*receiver), *message, *arguments, *block],
        Kind::ConstPathField { parent, constant } | Kind::ConstPathRef { parent, constant } => {
            vec![Some(*parent), Some(*constant)]
        }
        Kind::ConstRef { constant }
        | Kind::TopConstField { constant }
        | Kind::TopConstRef { constant } => vec![Some(*constant)],
        Kind::DefNode {
            target,
            operator,
            name,
            params,
            bodystmt,
        } => vec![
            *target,
            match operator {
                CallOperator::Node(id) => Some(*id),
                _ => None,
            },
            Some(*name),
            *params,
            Some(*bodystmt),
        ],
        Kind::Defined { value } => vec![Some(*value)],
        Kind::BlockNode {
            opening,
            block_var,
            bodystmt,
        } => vec![Some(*opening), *block_var, Some(*bodystmt)],
        Kind::RangeNode { left, right, .. } => vec![*left, *right],
        Kind::DynaSymbol { parts, .. }
        | Kind::Word { parts }
        | Kind::XStringLiteral { parts }
        | Kind::StringLiteral { parts, .. }
        | Kind::RegexpLiteral { parts, .. } => all(parts).collect(),
        Kind::Else {
            keyword,
            statements,
        }
        | Kind::Ensure {
            keyword,
            statements,
        } => vec![Some(*keyword), Some(*statements)],
        Kind::Elsif {
            predicate,
            statements,
            consequent,
        }
        | Kind::IfNode {
            predicate,
            statements,
            consequent,
        }
        | Kind::UnlessNode {
            predicate,
            statements,
            consequent,
        } => vec![Some(*predicate), Some(*statements), *consequent],
        Kind::Field {
            parent,
            operator,
            name,
        } => {
            let mut v = vec![Some(*parent)];
            if let Some(o) = op(operator) {
                v.push(o);
            }
            v.push(Some(*name));
            v
        }
        Kind::FndPtn {
            constant,
            left,
            values,
            right,
        } => {
            let mut v = vec![*constant, Some(*left)];
            v.extend(all(values));
            v.push(Some(*right));
            v
        }
        Kind::For {
            index,
            collection,
            statements,
        } => vec![Some(*index), Some(*collection), Some(*statements)],
        Kind::HashLiteral { lbrace, assocs } => {
            let mut v = vec![Some(*lbrace)];
            v.extend(all(assocs));
            v
        }
        Kind::Heredoc {
            beginning,
            ending,
            parts,
            ..
        } => {
            let mut v = vec![Some(*beginning)];
            v.extend(all(parts));
            v.push(*ending);
            v
        }
        Kind::HshPtn {
            constant,
            keywords,
            keyword_rest,
        } => {
            let mut v = vec![*constant];
            for (key, value) in keywords {
                v.push(Some(*key));
                v.push(*value);
            }
            v.push(*keyword_rest);
            v
        }
        Kind::IfOp {
            predicate,
            truthy,
            falsy,
        } => vec![Some(*predicate), Some(*truthy), Some(*falsy)],
        Kind::In {
            pattern,
            statements,
            consequent,
        } => vec![Some(*pattern), Some(*statements), *consequent],
        Kind::Lambda { params, statements } => vec![Some(*params), Some(*statements)],
        Kind::MethodAddBlock { call, block } => vec![Some(*call), Some(*block)],
        Kind::MLHS { parts, .. } | Kind::MRHS { parts } => all(parts).collect(),
        Kind::MLHSParen { contents, .. } => vec![Some(*contents)],
        Kind::ModuleDeclaration { constant, bodystmt } => {
            vec![Some(*constant), Some(*bodystmt)]
        }
        Kind::OpAssign {
            target,
            operator,
            value,
        } => vec![Some(*target), Some(*operator), Some(*value)],
        Kind::Params {
            requireds,
            optionals,
            rest,
            posts,
            keywords,
            keyword_rest,
            block,
        } => {
            let mut v: Vec<Option<NodeId>> = all(requireds).collect();
            for (name, value) in optionals {
                v.push(Some(*name));
                v.push(Some(*value));
            }
            v.push(*rest);
            v.extend(all(posts));
            for (name, value) in keywords {
                v.push(Some(*name));
                v.push(*value);
            }
            match keyword_rest {
                NilOr::NilSymbol => {}
                NilOr::None => v.push(None),
                NilOr::Node(id) => v.push(Some(*id)),
            }
            v.push(*block);
            v
        }
        Kind::Paren { lparen, contents } => vec![Some(*lparen), *contents],
        Kind::Program { statements } => vec![Some(*statements)],
        Kind::RescueEx {
            exceptions,
            variable,
        } => vec![*exceptions, *variable],
        Kind::Rescue {
            keyword,
            exception,
            statements,
            consequent,
        } => vec![Some(*keyword), *exception, Some(*statements), *consequent],
        Kind::RescueMod { statement, value } => vec![Some(*statement), Some(*value)],
        Kind::ReturnNode { arguments } | Kind::YieldNode { arguments } => vec![*arguments],
        Kind::SClass { target, bodystmt } => vec![Some(*target), Some(*bodystmt)],
        Kind::Statements { body } => all(body).collect(),
        Kind::StringConcat { left, right } => vec![Some(*left), Some(*right)],
        Kind::StringDVar { variable } => vec![Some(*variable)],
        Kind::StringEmbExpr { statements } => vec![Some(*statements)],
        Kind::Super { arguments } => vec![Some(*arguments)],
        Kind::SymbolLiteral { value } => vec![Some(*value)],
        Kind::Not { statement, .. } => vec![*statement],
        Kind::Unary { statement, .. } => vec![Some(*statement)],
        Kind::Undef { symbols } => all(symbols).collect(),
        Kind::UntilNode {
            predicate,
            statements,
        }
        | Kind::WhileNode {
            predicate,
            statements,
        } => vec![Some(*predicate), Some(*statements)],
        Kind::VarField { value } => match value {
            NilOr::NilSymbol => vec![],
            NilOr::None => vec![None],
            NilOr::Node(id) => vec![Some(*id)],
        },
        Kind::VarRef { value } | Kind::PinnedVarRef { value } | Kind::VCall { value } => {
            vec![Some(*value)]
        }
        Kind::When {
            arguments,
            statements,
            consequent,
        } => vec![Some(*arguments), Some(*statements), *consequent],
        _ => vec![],
    }
}

enum Field {
    Node(NodeId),
    Nil,
    Str(String),
    Raw(String),
    List(Vec<Field>),
}

pub fn kind_name(kind: &Kind) -> &'static str {
    match kind {
        Kind::BEGINBlock { .. } => "BEGINBlock",
        Kind::CHAR { .. } => "CHAR",
        Kind::ENDBlock { .. } => "ENDBlock",
        Kind::EndContent { .. } => "EndContent",
        Kind::AliasNode { .. } => "AliasNode",
        Kind::ARef { .. } => "ARef",
        Kind::ARefField { .. } => "ARefField",
        Kind::ArgParen { .. } => "ArgParen",
        Kind::Args { .. } => "Args",
        Kind::ArgBlock { .. } => "ArgBlock",
        Kind::ArgStar { .. } => "ArgStar",
        Kind::ArgsForward => "ArgsForward",
        Kind::ArrayLiteral { .. } => "ArrayLiteral",
        Kind::AryPtn { .. } => "AryPtn",
        Kind::Assign { .. } => "Assign",
        Kind::Assoc { .. } => "Assoc",
        Kind::AssocSplat { .. } => "AssocSplat",
        Kind::Backref { .. } => "Backref",
        Kind::Backtick { .. } => "Backtick",
        Kind::BareAssocHash { .. } => "BareAssocHash",
        Kind::Begin { .. } => "Begin",
        Kind::PinnedBegin { .. } => "PinnedBegin",
        Kind::Binary { .. } => "Binary",
        Kind::BlockVar { .. } => "BlockVar",
        Kind::BlockArg { .. } => "BlockArg",
        Kind::BodyStmt { .. } => "BodyStmt",
        Kind::Break { .. } => "Break",
        Kind::CallNode { .. } => "CallNode",
        Kind::Case { .. } => "Case",
        Kind::RAssign { .. } => "RAssign",
        Kind::ClassDeclaration { .. } => "ClassDeclaration",
        Kind::Comma { .. } => "Comma",
        Kind::Command { .. } => "Command",
        Kind::CommandCall { .. } => "CommandCall",
        Kind::Comment { .. } => "Comment",
        Kind::Const { .. } => "Const",
        Kind::ConstPathField { .. } => "ConstPathField",
        Kind::ConstPathRef { .. } => "ConstPathRef",
        Kind::ConstRef { .. } => "ConstRef",
        Kind::CVar { .. } => "CVar",
        Kind::DefNode { .. } => "DefNode",
        Kind::Defined { .. } => "Defined",
        Kind::BlockNode { .. } => "BlockNode",
        Kind::RangeNode { .. } => "RangeNode",
        Kind::DynaSymbol { .. } => "DynaSymbol",
        Kind::Else { .. } => "Else",
        Kind::Elsif { .. } => "Elsif",
        Kind::EmbDoc { .. } => "EmbDoc",
        Kind::Ensure { .. } => "Ensure",
        Kind::ExcessedComma { .. } => "ExcessedComma",
        Kind::Field { .. } => "Field",
        Kind::FloatLiteral { .. } => "FloatLiteral",
        Kind::FndPtn { .. } => "FndPtn",
        Kind::For { .. } => "For",
        Kind::GVar { .. } => "GVar",
        Kind::HashLiteral { .. } => "HashLiteral",
        Kind::Heredoc { .. } => "Heredoc",
        Kind::HeredocBeg { .. } => "HeredocBeg",
        Kind::HeredocEnd { .. } => "HeredocEnd",
        Kind::HshPtn { .. } => "HshPtn",
        Kind::Ident { .. } => "Ident",
        Kind::IfNode { .. } => "IfNode",
        Kind::IfOp { .. } => "IfOp",
        Kind::Imaginary { .. } => "Imaginary",
        Kind::In { .. } => "In",
        Kind::Int { .. } => "Int",
        Kind::IVar { .. } => "IVar",
        Kind::Kw { .. } => "Kw",
        Kind::KwRestParam { .. } => "KwRestParam",
        Kind::Label { .. } => "Label",
        Kind::Lambda { .. } => "Lambda",
        Kind::LambdaVar { .. } => "LambdaVar",
        Kind::LBrace { .. } => "LBrace",
        Kind::LBracket { .. } => "LBracket",
        Kind::LParen { .. } => "LParen",
        Kind::MAssign { .. } => "MAssign",
        Kind::MethodAddBlock { .. } => "MethodAddBlock",
        Kind::MLHS { .. } => "MLHS",
        Kind::MLHSParen { .. } => "MLHSParen",
        Kind::ModuleDeclaration { .. } => "ModuleDeclaration",
        Kind::MRHS { .. } => "MRHS",
        Kind::Next { .. } => "Next",
        Kind::Op { .. } => "Op",
        Kind::OpAssign { .. } => "OpAssign",
        Kind::Params { .. } => "Params",
        Kind::Paren { .. } => "Paren",
        Kind::Period { .. } => "Period",
        Kind::Program { .. } => "Program",
        Kind::QSymbols { .. } => "QSymbols",
        Kind::QSymbolsBeg { .. } => "QSymbolsBeg",
        Kind::QWords { .. } => "QWords",
        Kind::QWordsBeg { .. } => "QWordsBeg",
        Kind::RationalLiteral { .. } => "RationalLiteral",
        Kind::Redo => "Redo",
        Kind::RegexpLiteral { .. } => "RegexpLiteral",
        Kind::RescueEx { .. } => "RescueEx",
        Kind::Rescue { .. } => "Rescue",
        Kind::RescueMod { .. } => "RescueMod",
        Kind::RestParam { .. } => "RestParam",
        Kind::Retry => "Retry",
        Kind::ReturnNode { .. } => "ReturnNode",
        Kind::SClass { .. } => "SClass",
        Kind::Statements { .. } => "Statements",
        Kind::StringConcat { .. } => "StringConcat",
        Kind::StringDVar { .. } => "StringDVar",
        Kind::StringEmbExpr { .. } => "StringEmbExpr",
        Kind::StringLiteral { .. } => "StringLiteral",
        Kind::Super { .. } => "Super",
        Kind::SymbolLiteral { .. } => "SymbolLiteral",
        Kind::Symbols { .. } => "Symbols",
        Kind::SymbolsBeg { .. } => "SymbolsBeg",
        Kind::TopConstField { .. } => "TopConstField",
        Kind::TopConstRef { .. } => "TopConstRef",
        Kind::TStringContent { .. } => "TStringContent",
        Kind::Not { .. } => "Not",
        Kind::Unary { .. } => "Unary",
        Kind::Undef { .. } => "Undef",
        Kind::UnlessNode { .. } => "UnlessNode",
        Kind::UntilNode { .. } => "UntilNode",
        Kind::VarField { .. } => "VarField",
        Kind::VarRef { .. } => "VarRef",
        Kind::PinnedVarRef { .. } => "PinnedVarRef",
        Kind::VCall { .. } => "VCall",
        Kind::VoidStmt => "VoidStmt",
        Kind::When { .. } => "When",
        Kind::WhileNode { .. } => "WhileNode",
        Kind::Word { .. } => "Word",
        Kind::Words { .. } => "Words",
        Kind::WordsBeg { .. } => "WordsBeg",
        Kind::XStringLiteral { .. } => "XStringLiteral",
        Kind::YieldNode { .. } => "YieldNode",
        Kind::ZSuper => "ZSuper",
    }
}

/// `String#inspect` for the characters that show up in practice.
pub fn ruby_inspect(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{1b}' => out.push_str("\\e"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '#' => {
                if matches!(chars.peek(), Some('{') | Some('$') | Some('@')) {
                    out.push_str("\\#");
                } else {
                    out.push('#');
                }
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\x{:02X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn ruby_symbol(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let operator = matches!(
        name,
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "**"
            | "=="
            | "==="
            | "!="
            | "=~"
            | "!~"
            | "<"
            | "<="
            | ">"
            | ">="
            | "<=>"
            | "<<"
            | ">>"
            | "&"
            | "|"
            | "^"
            | "!"
    );
    if plain || operator {
        format!(":{name}")
    } else {
        format!(":{}", ruby_inspect(name))
    }
}
