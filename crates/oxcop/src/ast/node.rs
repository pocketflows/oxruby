//! Port of `RuboCop::AST::Node` and its subclasses' helper methods
//! (`node/*.rb` and `node/mixin/*.rb` in rubocop-ast). One handle type
//! serves every node type; helpers that only exist on some node classes in
//! Ruby dispatch on the node type and return `None` (or a neutral value)
//! elsewhere.
//!
//! `==` on handles is identity (`equal?`); structural equality (Ruby's
//! `==` on nodes) is [`Node::same_as`].

use crate::ast::{Ast, Child, Loc, MapKind, NodeData, NodeId, NodeType};
use crate::processed_source::Range;

#[derive(Clone, Copy)]
pub struct Node<'a> {
    ast: &'a Ast,
    id: NodeId,
}

impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && std::ptr::eq(self.ast, other.ast)
    }
}

impl Eq for Node<'_> {}

impl std::hash::Hash for Node<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl std::fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Node({} {:?})", self.ty().name(), self.source())
    }
}

pub const COMPARISON_OPERATORS: &[&str] = &["==", "===", "!=", "<=", ">=", ">", "<"];

const OPERATOR_METHODS: &[&str] = &[
    "|", "^", "&", "<=>", "==", "===", "=~", ">", ">=", "<", "<=", "<<", ">>", "+", "-", "*", "/",
    "%", "**", "~", "+@", "-@", "!@", "~@", "[]", "[]=", "!", "!=", "!~", "`",
];

const ENUMERATOR_METHODS: &[&str] = &[
    "collect",
    "collect_concat",
    "detect",
    "downto",
    "each",
    "find",
    "find_all",
    "find_index",
    "inject",
    "loop",
    "map!",
    "map",
    "reduce",
    "reject",
    "reject!",
    "reverse_each",
    "select",
    "select!",
    "times",
    "upto",
];

/// `Enumerable.instance_methods + [:each]` (Ruby 3.3).
const ENUMERABLE_METHODS: &[&str] = &[
    "all?",
    "any?",
    "chain",
    "chunk",
    "chunk_while",
    "collect",
    "collect_concat",
    "compact",
    "count",
    "cycle",
    "detect",
    "drop",
    "drop_while",
    "each",
    "each_cons",
    "each_entry",
    "each_slice",
    "each_with_index",
    "each_with_object",
    "entries",
    "filter",
    "filter_map",
    "find",
    "find_all",
    "find_index",
    "first",
    "flat_map",
    "grep",
    "grep_v",
    "group_by",
    "include?",
    "inject",
    "lazy",
    "map",
    "max",
    "max_by",
    "member?",
    "min",
    "min_by",
    "minmax",
    "minmax_by",
    "none?",
    "one?",
    "partition",
    "reduce",
    "reject",
    "reverse_each",
    "select",
    "slice_after",
    "slice_before",
    "slice_when",
    "sort",
    "sort_by",
    "sum",
    "take",
    "take_while",
    "tally",
    "to_a",
    "to_h",
    "to_set",
    "uniq",
    "zip",
];

const ARITHMETIC_OPERATORS: &[&str] = &["+", "-", "*", "/", "%", "**"];

/// Value types for the `*_literal?` predicates.
fn truthy_literal(ty: NodeType) -> bool {
    use NodeType::*;
    matches!(
        ty,
        Str | Dstr
            | Xstr
            | Int
            | Float
            | Sym
            | Dsym
            | Array
            | Hash
            | Regexp
            | True
            | Irange
            | Erange
            | Complex
            | Rational
            | Regopt
    )
}

fn falsey_literal(ty: NodeType) -> bool {
    matches!(ty, NodeType::False | NodeType::Nil)
}

fn composite_literal(ty: NodeType) -> bool {
    use NodeType::*;
    matches!(
        ty,
        Dstr | Xstr | Dsym | Array | Hash | Irange | Erange | Regexp
    )
}

fn mutable_literal(ty: NodeType) -> bool {
    use NodeType::*;
    matches!(
        ty,
        Str | Dstr | Xstr | Array | Hash | Regexp | Irange | Erange
    )
}

impl<'a> Node<'a> {
    pub(crate) fn new(ast: &'a Ast, id: NodeId) -> Self {
        Self { ast, id }
    }

    pub fn ast(&self) -> &'a Ast {
        self.ast
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    fn data(&self) -> &'a NodeData {
        self.ast.data(self.id)
    }

    fn node(&self, id: NodeId) -> Node<'a> {
        Node::new(self.ast, id)
    }

    pub fn ty(&self) -> NodeType {
        self.data().ty
    }

    pub fn is(&self, ty: NodeType) -> bool {
        self.ty() == ty
    }

    /// `type?(*types)`, with group types (`call`, `any_block`, ...).
    pub fn is_any(&self, types: &[NodeType]) -> bool {
        types.contains(&self.ty())
    }

    pub fn loc(&self) -> &'a Loc {
        &self.data().loc
    }

    pub fn children(&self) -> &'a [Child] {
        &self.data().children
    }

    /// The `index`th child if it is a node.
    pub fn child(&self, index: usize) -> Option<Node<'a>> {
        self.children()
            .get(index)
            .and_then(Child::as_node)
            .map(|id| self.node(id))
    }

    /// The child at `index` counting from the end (`children[-1]` is 1).
    pub fn child_from_end(&self, index_from_end: usize) -> Option<Node<'a>> {
        let len = self.children().len();
        if index_from_end == 0 || index_from_end > len {
            return None;
        }
        self.child(len - index_from_end)
    }

    pub fn child_value(&self, index: usize) -> Option<&'a Child> {
        self.children().get(index)
    }

    /// A symbol child (`children[index]` when it is a `Symbol`).
    pub fn child_sym(&self, index: usize) -> Option<&'a str> {
        self.children().get(index).and_then(Child::as_sym)
    }

    /// `each_child_node` / `child_nodes`: child nodes, skipping `nil` and
    /// literal values.
    pub fn child_nodes(&self) -> impl DoubleEndedIterator<Item = Node<'a>> + 'a {
        let ast = self.ast;
        self.children()
            .iter()
            .filter_map(Child::as_node)
            .map(move |id| Node::new(ast, id))
    }

    pub fn parent(&self) -> Option<Node<'a>> {
        self.data().parent.map(|id| self.node(id))
    }

    pub fn is_root(&self) -> bool {
        self.data().parent.is_none()
    }

    /// Descendants in pre-order (`each_descendant`).
    pub fn descendants(&self) -> impl Iterator<Item = Node<'a>> + 'a {
        let ast = self.ast;
        (self.id + 1..self.data().subtree_end).map(move |id| Node::new(ast, id))
    }

    /// `each_node`: this node, then its descendants.
    pub fn each_node(&self) -> impl Iterator<Item = Node<'a>> + 'a {
        let ast = self.ast;
        (self.id..self.data().subtree_end).map(move |id| Node::new(ast, id))
    }

    /// Ancestors from the parent outwards (`each_ancestor`).
    pub fn ancestors(&self) -> impl Iterator<Item = Node<'a>> + 'a {
        std::iter::successors(self.parent(), |node| node.parent())
    }

    /// Whether `other` is this node or one of its descendants.
    pub fn contains_node(&self, other: Node<'_>) -> bool {
        other.id >= self.id && other.id < self.data().subtree_end
    }

    /// `node.source_range` / `loc.expression`.
    pub fn source_range(&self) -> Range {
        self.loc().expression.unwrap_or_default()
    }

    pub fn has_source(&self) -> bool {
        self.loc().expression.is_some()
    }

    /// `node.source`.
    pub fn source(&self) -> &'a str {
        match self.loc().expression {
            Some(r) => &self.ast.source()[r.begin_pos..r.end_pos],
            None => "",
        }
    }

    /// The source of a range in this node's file.
    pub fn range_source(&self, range: Range) -> &'a str {
        &self.ast.source()[range.begin_pos..range.end_pos]
    }

    /// `first_line` (`loc.line`).
    pub fn first_line(&self) -> usize {
        self.ast.line_of(self.source_range().begin_pos)
    }

    /// `last_line` (`loc.last_line`).
    pub fn last_line(&self) -> usize {
        self.ast.line_of(self.source_range().end_pos)
    }

    pub fn line_of(&self, range: Range) -> usize {
        self.ast.line_of(range.begin_pos)
    }

    pub fn last_line_of(&self, range: Range) -> usize {
        self.ast.line_of(range.end_pos)
    }

    pub fn column_of(&self, pos: usize) -> usize {
        self.ast.column_of(pos)
    }

    /// `loc.column`.
    pub fn column(&self) -> usize {
        self.ast.column_of(self.source_range().begin_pos)
    }

    /// `loc.last_column`.
    pub fn last_column(&self) -> usize {
        self.ast.column_of(self.source_range().end_pos)
    }

    pub fn line_count(&self) -> usize {
        if self.loc().expression.is_none() {
            return 0;
        }
        self.last_line() - self.first_line() + 1
    }

    pub fn is_multiline(&self) -> bool {
        match self.ty() {
            NodeType::Block | NodeType::Numblock | NodeType::Itblock => !self.block_single_line(),
            _ => self.line_count() > 1,
        }
    }

    pub fn is_single_line(&self) -> bool {
        match self.ty() {
            NodeType::Block | NodeType::Numblock | NodeType::Itblock => self.block_single_line(),
            _ => self.line_count() == 1,
        }
    }

    fn block_single_line(&self) -> bool {
        match (self.loc().begin, self.loc().end) {
            (Some(b), Some(e)) => self.ast.line_of(b.begin_pos) == self.ast.line_of(e.begin_pos),
            _ => self.line_count() == 1,
        }
    }

    pub fn source_length(&self) -> usize {
        self.source_range().len()
    }

    pub fn is_empty_source(&self) -> bool {
        self.source_length() == 0
    }

    /// `loc?(which)`: `respond_to?` and non-nil.
    pub fn loc_is(&self, range: Option<Range>, text: &str) -> bool {
        range.is_some_and(|r| self.range_source(r) == text)
    }

    /// Structural equality (Ruby's `==` on nodes).
    pub fn same_as(&self, other: Node<'_>) -> bool {
        if self.ty() != other.ty() {
            return false;
        }
        let (a, b) = (self.children(), other.children());
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| match (x, y) {
                (Child::Node(x), Child::Node(y)) => self.node(*x).same_as(Node::new(other.ast, *y)),
                (x, y) => x == y,
            })
    }

    // ---- siblings ------------------------------------------------------------

    pub fn sibling_index(&self) -> Option<usize> {
        let parent = self.parent()?;
        parent
            .children()
            .iter()
            .position(|c| c.as_node() == Some(self.id))
    }

    /// `right_sibling` when it is a node.
    pub fn right_sibling(&self) -> Option<Node<'a>> {
        let parent = self.parent()?;
        parent.child(self.sibling_index()? + 1)
    }

    pub fn right_sibling_value(&self) -> Option<&'a Child> {
        let parent = self.parent()?;
        parent.children().get(self.sibling_index()? + 1)
    }

    /// `left_sibling` when it is a node.
    pub fn left_sibling(&self) -> Option<Node<'a>> {
        let i = self.sibling_index()?;
        if i == 0 {
            return None;
        }
        self.parent()?.child(i - 1)
    }

    pub fn left_siblings(&self) -> Vec<Node<'a>> {
        match (self.parent(), self.sibling_index()) {
            (Some(p), Some(i)) => p.children()[..i]
                .iter()
                .filter_map(Child::as_node)
                .map(|id| self.node(id))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn right_siblings(&self) -> Vec<Node<'a>> {
        match (self.parent(), self.sibling_index()) {
            (Some(p), Some(i)) => p.children()[i + 1..]
                .iter()
                .filter_map(Child::as_node)
                .map(|id| self.node(id))
                .collect(),
            _ => Vec::new(),
        }
    }

    // ---- type predicates -------------------------------------------------------

    pub fn is_send(&self) -> bool {
        self.is(NodeType::Send)
    }

    pub fn is_csend(&self) -> bool {
        self.is(NodeType::Csend)
    }

    /// `call_type?`: `send` or `csend`.
    pub fn is_call(&self) -> bool {
        matches!(self.ty(), NodeType::Send | NodeType::Csend)
    }

    /// `any_block_type?`
    pub fn is_any_block(&self) -> bool {
        matches!(
            self.ty(),
            NodeType::Block | NodeType::Numblock | NodeType::Itblock
        )
    }

    /// `any_def_type?`
    pub fn is_any_def(&self) -> bool {
        matches!(self.ty(), NodeType::Def | NodeType::Defs)
    }

    /// `argument_type?`
    pub fn is_argument_type(&self) -> bool {
        use NodeType::*;
        matches!(
            self.ty(),
            Arg | Optarg
                | Restarg
                | Kwarg
                | Kwoptarg
                | Kwrestarg
                | Blockarg
                | ForwardArg
                | Shadowarg
        )
    }

    pub fn is_boolean(&self) -> bool {
        matches!(self.ty(), NodeType::True | NodeType::False)
    }

    pub fn is_numeric(&self) -> bool {
        use NodeType::*;
        matches!(self.ty(), Int | Float | Rational | Complex)
    }

    pub fn is_range(&self) -> bool {
        matches!(self.ty(), NodeType::Irange | NodeType::Erange)
    }

    pub fn is_any_str(&self) -> bool {
        matches!(self.ty(), NodeType::Str | NodeType::Dstr | NodeType::Xstr)
    }

    pub fn is_any_sym(&self) -> bool {
        matches!(self.ty(), NodeType::Sym | NodeType::Dsym)
    }

    pub fn is_any_match_pattern(&self) -> bool {
        matches!(self.ty(), NodeType::MatchPattern | NodeType::MatchPatternP)
    }

    /// `type_in?` for a type name as written in node patterns, including
    /// the group names.
    pub fn matches_type_name(&self, name: &str) -> bool {
        match name {
            "call" => self.is_call(),
            "any_block" => self.is_any_block(),
            "any_def" => self.is_any_def(),
            "argument" => self.is_argument_type(),
            "boolean" => self.is_boolean(),
            "numeric" => self.is_numeric(),
            "range" => self.is_range(),
            "any_str" => self.is_any_str(),
            "any_sym" => self.is_any_sym(),
            "any_match_pattern" => self.is_any_match_pattern(),
            // `send_type?` is false for `csend` nodes.
            name => self.ty().name() == name,
        }
    }

    pub fn is_literal(&self) -> bool {
        truthy_literal(self.ty()) || falsey_literal(self.ty())
    }

    pub fn is_basic_literal(&self) -> bool {
        self.is_literal() && !composite_literal(self.ty())
    }

    pub fn is_truthy_literal(&self) -> bool {
        truthy_literal(self.ty())
    }

    pub fn is_falsey_literal(&self) -> bool {
        falsey_literal(self.ty())
    }

    pub fn is_mutable_literal(&self) -> bool {
        mutable_literal(self.ty())
    }

    pub fn is_immutable_literal(&self) -> bool {
        self.is_literal() && !mutable_literal(self.ty())
    }

    fn recursive_predicate(&self, base: fn(&Node<'a>) -> bool) -> bool {
        const RECURSIVE_METHODS: &[&str] =
            &["==", "===", "!=", "<=", ">=", ">", "<", "*", "!", "<=>"];
        use NodeType::*;
        match self.ty() {
            Send => {
                self.method_name()
                    .is_some_and(|m| RECURSIVE_METHODS.contains(&m))
                    && self.receiver().is_some_and(|r| r.recursive_predicate(base))
                    && self.arguments().iter().all(|a| a.recursive_predicate(base))
            }
            And | Or | Dstr | Xstr | Dsym | Array | Hash | Irange | Erange | Regexp | Begin
            | Pair => self.child_nodes().all(|c| c.recursive_predicate(base)),
            _ => base(self),
        }
    }

    pub fn is_recursive_literal(&self) -> bool {
        self.recursive_predicate(|n| n.is_literal())
    }

    pub fn is_recursive_basic_literal(&self) -> bool {
        self.recursive_predicate(|n| n.is_basic_literal())
    }

    pub fn is_variable(&self) -> bool {
        use NodeType::*;
        matches!(self.ty(), Ivar | Gvar | Cvar | Lvar)
    }

    pub fn is_reference(&self) -> bool {
        matches!(self.ty(), NodeType::NthRef | NodeType::BackRef)
    }

    pub fn is_equals_asgn(&self) -> bool {
        use NodeType::*;
        matches!(self.ty(), Lvasgn | Ivasgn | Cvasgn | Gvasgn | Casgn | Masgn)
    }

    pub fn is_shorthand_asgn(&self) -> bool {
        use NodeType::*;
        matches!(self.ty(), OpAsgn | OrAsgn | AndAsgn)
    }

    /// `assignment?` on `Node` (not the `MethodDispatchNode` alias).
    pub fn is_assignment(&self) -> bool {
        self.is_equals_asgn() || self.is_shorthand_asgn()
    }

    pub fn is_basic_conditional(&self) -> bool {
        matches!(self.ty(), NodeType::If | NodeType::While | NodeType::Until)
    }

    pub fn is_conditional(&self) -> bool {
        self.is_basic_conditional() || matches!(self.ty(), NodeType::Case | NodeType::CaseMatch)
    }

    pub fn is_post_condition_loop(&self) -> bool {
        matches!(self.ty(), NodeType::WhilePost | NodeType::UntilPost)
    }

    pub fn is_loop_keyword(&self) -> bool {
        self.is_post_condition_loop()
            || matches!(self.ty(), NodeType::While | NodeType::Until | NodeType::For)
    }

    pub fn is_operator_keyword(&self) -> bool {
        matches!(self.ty(), NodeType::And | NodeType::Or)
    }

    /// `keyword?`
    pub fn is_keyword(&self) -> bool {
        use NodeType::*;
        if self.is_special_keyword() || (self.is_send() && self.is_prefix_not()) {
            return true;
        }
        let keyword_type = matches!(
            self.ty(),
            Alias
                | And
                | Break
                | Case
                | Class
                | Def
                | Defs
                | Defined
                | Kwbegin
                | For
                | If
                | Module
                | Next
                | Not
                | Or
                | Postexe
                | Redo
                | Rescue
                | Retry
                | Return
                | SelfNode
                | Super
                | Zsuper
                | Undef
                | Until
                | When
                | While
                | Yield
        );
        if !keyword_type {
            return false;
        }
        !self.is_operator_keyword()
            || self
                .loc()
                .operator
                .is_some_and(|op| self.range_source(op) == self.ty().name())
    }

    pub fn is_special_keyword(&self) -> bool {
        use NodeType::*;
        matches!(self.ty(), Str | Sym | Int | Const | File | Line | Encoding)
            && matches!(self.source(), "__FILE__" | "__LINE__" | "__ENCODING__")
    }

    // ---- values ------------------------------------------------------------------

    /// `str_content`: the value of a `str` node.
    pub fn str_content(&self) -> Option<&'a str> {
        if self.ty() != NodeType::Str {
            return None;
        }
        self.children().first().and_then(Child::as_str)
    }

    /// `value` of `str`/`sym`/`int`/`float`... (`BasicLiteralNode#value`).
    pub fn value(&self) -> Option<&'a Child> {
        self.children().first()
    }

    /// The symbol of a `sym` node.
    pub fn sym_value(&self) -> Option<&'a str> {
        if self.ty() != NodeType::Sym {
            return None;
        }
        self.child_sym(0)
    }

    /// The value of an `int` node.
    pub fn int_value(&self) -> Option<i64> {
        match (self.ty(), self.children().first()) {
            (NodeType::Int, Some(Child::Int(i))) => Some(*i),
            _ => None,
        }
    }

    /// `name` of variables, assignments and arguments (`VarNode`,
    /// `AsgnNode`, `ArgNode`, `CasgnNode`, `Procarg0Node`).
    pub fn name(&self) -> Option<&'a str> {
        use NodeType::*;
        match self.ty() {
            Lvar | Ivar | Cvar | Gvar | Lvasgn | Ivasgn | Cvasgn | Gvasgn | Arg | Optarg
            | Restarg | Kwarg | Kwoptarg | Kwrestarg | Blockarg | Shadowarg | MatchVar => {
                self.child_sym(0)
            }
            Casgn => self.child_sym(1),
            Procarg0 => self
                .child(0)
                .and_then(|c| c.name())
                .or_else(|| self.child_sym(0)),
            OpAsgn | OrAsgn | AndAsgn => {
                let lhs = self.child(0)?;
                if lhs.is_call() {
                    lhs.method_name()
                } else {
                    lhs.name()
                }
            }
            _ => None,
        }
    }

    // ---- method dispatch (`SendNode`, `MethodDispatchNode`) -----------------

    /// `receiver`. For `send`/`csend` the first child; for `def`/`defs` the
    /// definee; for blocks, the send node's receiver (`Node#receiver`).
    pub fn receiver(&self) -> Option<Node<'a>> {
        use NodeType::*;
        match self.ty() {
            Send | Csend => self.child(0),
            Defs => self.child(0),
            Def => None,
            Block | Numblock | Itblock => self
                .child(0)
                .and_then(|s| if s.is_call() { s.child(0) } else { None }),
            _ => None,
        }
    }

    /// `method_name`.
    pub fn method_name(&self) -> Option<&'a str> {
        use NodeType::*;
        match self.ty() {
            Send | Csend => self.child_sym(1),
            Super | Zsuper => Some("super"),
            Yield => Some("yield"),
            Defined => Some("defined?"),
            Lambda => Some("lambda"),
            Index => Some("[]"),
            Indexasgn => Some("[]="),
            Def => self.child_sym(0),
            Defs => self.child_sym(1),
            Block | Numblock | Itblock => self.child(0).and_then(|s| s.method_name()),
            _ => None,
        }
    }

    /// `method?(name)`.
    pub fn is_method(&self, name: &str) -> bool {
        self.method_name() == Some(name)
    }

    /// `arguments` as nodes. For calls (`send`, `super`, `yield`, ...), the
    /// argument nodes; for `def`/`defs` and `block`, the children of the
    /// `args` node; for `return`/`break`/`next`, the wrapped arguments.
    pub fn arguments(&self) -> Vec<Node<'a>> {
        use NodeType::*;
        match self.ty() {
            Send | Csend | Lambda => self.children()[2.min(self.children().len())..]
                .iter()
                .filter_map(Child::as_node)
                .map(|id| self.node(id))
                .collect(),
            Index | Indexasgn => self.children()[1.min(self.children().len())..]
                .iter()
                .filter_map(Child::as_node)
                .map(|id| self.node(id))
                .collect(),
            Super | Zsuper | Yield | Defined => self.child_nodes().collect(),
            Def => self
                .child(1)
                .map(|a| a.child_nodes().collect())
                .unwrap_or_default(),
            Defs => self
                .child(2)
                .map(|a| a.child_nodes().collect())
                .unwrap_or_default(),
            Block => self
                .child(1)
                .map(|a| a.child_nodes().collect())
                .unwrap_or_default(),
            Numblock | Itblock => Vec::new(),
            Return | Break | Next => match self.child(0) {
                Some(first) if first.is(Begin) => first.child_nodes().collect(),
                _ => self.child_nodes().collect(),
            },
            _ => Vec::new(),
        }
    }

    /// The `args` node of a `def`, `defs` or `block`.
    pub fn arguments_node(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::Def => self.child(1),
            NodeType::Defs => self.child(2),
            NodeType::Block => self.child(1),
            _ => None,
        }
    }

    pub fn first_argument(&self) -> Option<Node<'a>> {
        self.arguments().into_iter().next()
    }

    pub fn last_argument(&self) -> Option<Node<'a>> {
        self.arguments().into_iter().last()
    }

    /// `arguments?`
    pub fn has_arguments(&self) -> bool {
        !self.arguments().is_empty()
    }

    /// `parenthesized?` (`ParameterizedNode`): `loc.end` is `)`.
    pub fn is_parenthesized(&self) -> bool {
        self.loc_is(self.loc().end, ")")
    }

    /// `parenthesized_call?`: `loc.begin` is `(`.
    pub fn is_parenthesized_call(&self) -> bool {
        self.loc_is(self.loc().begin, "(")
    }

    pub fn is_splat_argument(&self) -> bool {
        self.arguments()
            .iter()
            .any(|a| matches!(a.ty(), NodeType::Splat | NodeType::Restarg))
    }

    pub fn is_block_argument(&self) -> bool {
        self.last_argument()
            .is_some_and(|a| matches!(a.ty(), NodeType::BlockPass | NodeType::Blockarg))
    }

    /// `selector`: `loc.keyword` or `loc.selector`.
    pub fn selector(&self) -> Option<Range> {
        self.loc().keyword.or(self.loc().selector)
    }

    /// `block_node`: the block this call is the send node of.
    pub fn block_node(&self) -> Option<Node<'a>> {
        if self.is_block_literal() {
            self.parent()
        } else {
            None
        }
    }

    /// `block_literal?`
    pub fn is_block_literal(&self) -> bool {
        self.parent()
            .is_some_and(|p| p.is_any_block() && p.send_node() == Some(*self))
    }

    /// `send_node` of a block.
    pub fn send_node(&self) -> Option<Node<'a>> {
        if self.is_any_block() {
            self.child(0)
        } else {
            None
        }
    }

    pub fn is_operator_method(&self) -> bool {
        self.method_name()
            .is_some_and(|m| OPERATOR_METHODS.contains(&m))
    }

    pub fn is_comparison_method(&self) -> bool {
        self.method_name()
            .is_some_and(|m| COMPARISON_OPERATORS.contains(&m))
    }

    /// `assignment_method?`
    pub fn is_assignment_method(&self) -> bool {
        match self.ty() {
            NodeType::Index | NodeType::Lambda => false,
            NodeType::Indexasgn => true,
            _ => {
                !self.is_comparison_method() && self.method_name().is_some_and(|m| m.ends_with('='))
            }
        }
    }

    pub fn is_enumerator_method(&self) -> bool {
        self.method_name()
            .is_some_and(|m| ENUMERATOR_METHODS.contains(&m) || m.starts_with("each_"))
    }

    pub fn is_enumerable_method(&self) -> bool {
        self.method_name()
            .is_some_and(|m| ENUMERABLE_METHODS.contains(&m))
    }

    pub fn is_predicate_method(&self) -> bool {
        self.method_name().is_some_and(|m| m.ends_with('?'))
    }

    pub fn is_bang_method(&self) -> bool {
        self.method_name().is_some_and(|m| m.ends_with('!'))
    }

    pub fn is_camel_case_method(&self) -> bool {
        self.method_name()
            .is_some_and(|m| m.starts_with(|c: char| c.is_ascii_uppercase()))
    }

    pub fn is_self_receiver(&self) -> bool {
        self.receiver().is_some_and(|r| r.is(NodeType::SelfNode))
    }

    pub fn is_const_receiver(&self) -> bool {
        self.receiver().is_some_and(|r| r.is(NodeType::Const))
    }

    pub fn is_negation_method(&self) -> bool {
        self.receiver().is_some() && self.method_name() == Some("!")
    }

    pub fn is_prefix_not(&self) -> bool {
        self.is_negation_method() && self.loc_is(self.loc().selector, "not")
    }

    pub fn is_prefix_bang(&self) -> bool {
        self.is_negation_method() && self.loc_is(self.loc().selector, "!")
    }

    /// `setter_method?` / `MethodDispatchNode#assignment?`.
    pub fn is_setter_method(&self) -> bool {
        self.loc().operator.is_some()
            && matches!(
                self.loc().kind,
                MapKind::Send | MapKind::Index | MapKind::Variable | MapKind::Constant
            )
    }

    pub fn is_dot(&self) -> bool {
        self.loc_is(self.loc().dot, ".")
    }

    pub fn is_double_colon(&self) -> bool {
        self.loc_is(self.loc().dot, "::")
    }

    pub fn is_safe_navigation(&self) -> bool {
        self.loc_is(self.loc().dot, "&.")
    }

    pub fn is_implicit_call(&self) -> bool {
        self.is_method("call") && self.selector().is_none()
    }

    pub fn is_arithmetic_operation(&self) -> bool {
        self.method_name()
            .is_some_and(|m| ARITHMETIC_OPERATORS.contains(&m))
    }

    /// `command?(name)`
    pub fn is_command(&self, name: &str) -> bool {
        self.receiver().is_none() && self.is_method(name)
    }

    /// `lambda?`: a block literal of `lambda`, or a block of `lambda`.
    pub fn is_lambda(&self) -> bool {
        if self.is_any_block() {
            return self.send_node().is_some_and(|s| s.is_method("lambda"));
        }
        if self.is(NodeType::Lambda) {
            return true;
        }
        self.is_block_literal() && self.is_command("lambda")
    }

    /// `lambda_literal?`: `-> {}`.
    pub fn is_lambda_literal(&self) -> bool {
        if self.is(NodeType::Lambda) {
            return true;
        }
        self.source() == "->" && self.is_block_literal()
    }

    pub fn is_unary_operation(&self) -> bool {
        match self.selector() {
            None => false,
            Some(selector) => {
                self.is_operator_method() && self.source_range().begin_pos == selector.begin_pos
            }
        }
    }

    pub fn is_binary_operation(&self) -> bool {
        match self.selector() {
            None => false,
            Some(selector) => {
                self.is_operator_method() && self.source_range().begin_pos != selector.begin_pos
            }
        }
    }

    /// `macro?`: no receiver and in a macro scope.
    pub fn is_macro(&self) -> bool {
        self.receiver().is_none() && self.in_macro_scope()
    }

    fn in_macro_scope(&self) -> bool {
        let Some(parent) = self.parent() else {
            return true;
        };
        use NodeType::*;
        if matches!(parent.ty(), Sclass | Class | Module) || parent.is_class_constructor() {
            return true;
        }
        let wrapper = matches!(parent.ty(), Kwbegin | Begin)
            || parent.is_any_block()
            || (parent.is(If) && parent.child(0) != Some(*self));
        wrapper && parent.in_macro_scope()
    }

    pub fn is_access_modifier(&self) -> bool {
        self.is_bare_access_modifier() || self.is_non_bare_access_modifier()
    }

    pub fn is_bare_access_modifier(&self) -> bool {
        self.is_macro()
            && self.is_send()
            && self.receiver().is_none()
            && matches!(
                self.method_name(),
                Some("public" | "protected" | "private" | "module_function")
            )
            && self.children().len() == 2
    }

    pub fn is_non_bare_access_modifier(&self) -> bool {
        self.is_macro()
            && self.is_send()
            && self.receiver().is_none()
            && matches!(
                self.method_name(),
                Some("public" | "protected" | "private" | "module_function")
            )
            && self.children().len() > 2
    }

    pub fn is_special_modifier(&self) -> bool {
        self.is_bare_access_modifier() && matches!(self.source(), "private" | "protected")
    }

    /// `def_modifier?`
    pub fn is_def_modifier(&self) -> bool {
        self.def_modifier().is_some()
    }

    pub fn def_modifier(&self) -> Option<Node<'a>> {
        if !self.is_send() || self.receiver().is_some() {
            return None;
        }
        let arg = self.child(2)?;
        if arg.is_any_def() {
            return Some(arg);
        }
        arg.def_modifier()
    }

    /// `attribute_accessor?`: `attr_reader :foo` and friends.
    pub fn attribute_accessor(&self) -> Option<(&'a str, Vec<Node<'a>>)> {
        if !self.is_send() || self.receiver().is_some() {
            return None;
        }
        let name = self.method_name()?;
        if !matches!(
            name,
            "attr_reader" | "attr_writer" | "attr_accessor" | "attr"
        ) {
            return None;
        }
        if self.children().len() < 3 {
            return None;
        }
        Some((name, self.arguments()))
    }

    /// `chained?`
    pub fn is_chained(&self) -> bool {
        self.parent()
            .is_some_and(|p| p.is_call() && p.receiver() == Some(*self))
    }

    /// `argument?`
    pub fn is_argument(&self) -> bool {
        self.parent()
            .is_some_and(|p| p.is_send() && p.arguments().contains(self))
    }

    // ---- def ---------------------------------------------------------------------

    /// `body` of `def`, `block`, `class`, `module`, `if`, `while`, ...
    pub fn body(&self) -> Option<Node<'a>> {
        use NodeType::*;
        match self.ty() {
            Def => self.child(2),
            Defs => self.child(3),
            Block | Numblock | Itblock => self.child(2),
            Class => self.child(2),
            Module | Sclass => self.child(1),
            If => self.if_branch(),
            While | Until | WhilePost | UntilPost => self.child(1),
            For => self.child(2),
            When | InPattern | Resbody => self.child_from_end(1),
            Rescue => self.child(0),
            Kwbegin => self.kwbegin_body(),
            Case | CaseMatch => self.child(1),
            _ => None,
        }
    }

    fn kwbegin_body(&self) -> Option<Node<'a>> {
        let first = self.child(0);
        if self.children().is_empty() {
            return None;
        }
        if let Some(rescue) = self.kwbegin_rescue_node() {
            return rescue.child(0);
        }
        if let Some(ensure) = first.filter(|f| f.is(NodeType::Ensure)) {
            return ensure.child(0);
        }
        if self.children().len() == 1 {
            return first;
        }
        Some(*self)
    }

    fn kwbegin_rescue_node(&self) -> Option<Node<'a>> {
        let first = self.child(0)?;
        if first.is(NodeType::Ensure)
            && let Some(rescue) = first.child(0).filter(|r| r.is(NodeType::Rescue))
        {
            return Some(rescue);
        }
        first.is(NodeType::Rescue).then_some(first)
    }

    /// `endless?` for `def`/`defs`.
    pub fn is_endless(&self) -> bool {
        self.is_any_def() && self.loc().end.is_none()
    }

    /// `void_context?` for `def`.
    pub fn is_void_context(&self) -> bool {
        match self.ty() {
            NodeType::Def | NodeType::Defs => {
                (self.is(NodeType::Def) && self.is_method("initialize"))
                    || self.is_assignment_method()
            }
            NodeType::Block | NodeType::Numblock | NodeType::Itblock => {
                matches!(self.method_name(), Some("each" | "tap"))
            }
            NodeType::For | NodeType::Ensure => true,
            _ => false,
        }
    }

    /// `argument_forwarding?`
    pub fn is_argument_forwarding(&self) -> bool {
        self.arguments()
            .iter()
            .any(|a| matches!(a.ty(), NodeType::ForwardArgs | NodeType::ForwardArg))
    }

    // ---- blocks ------------------------------------------------------------------

    /// `braces?` (blocks and hashes).
    pub fn has_braces(&self) -> bool {
        self.loc_is(self.loc().end, "}")
    }

    /// `keywords?` (blocks: `do ... end`).
    pub fn has_keywords(&self) -> bool {
        self.loc_is(self.loc().end, "end")
    }

    /// `argument_list` of a block: explicit args, numbered params or `it`.
    pub fn block_argument_names(&self) -> Vec<String> {
        match self.ty() {
            NodeType::Numblock => {
                let max = match self.children().get(1) {
                    Some(Child::Int(i)) => *i,
                    _ => 0,
                };
                (1..=max).map(|i| format!("_{i}")).collect()
            }
            NodeType::Itblock => vec!["it".to_owned()],
            NodeType::Block => self
                .child(1)
                .map(|args| {
                    args.descendants()
                        .filter(|d| d.is_argument_type())
                        .filter_map(|d| d.name().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    // ---- conditionals ----------------------------------------------------------

    /// `condition` of `if`, `while`, `until`, `case`.
    pub fn condition(&self) -> Option<Node<'a>> {
        use NodeType::*;
        match self.ty() {
            If | While | Until | WhilePost | UntilPost | Case | CaseMatch => self.child(0),
            _ => None,
        }
    }

    /// `if?`, `unless?`, `elsif?`: the `if` node's keyword.
    pub fn keyword(&self) -> &'a str {
        match self.ty() {
            NodeType::If => {
                if self.is_ternary() {
                    ""
                } else {
                    self.loc().keyword.map_or("", |k| self.range_source(k))
                }
            }
            NodeType::Case | NodeType::CaseMatch => "case",
            NodeType::For => "for",
            NodeType::While | NodeType::WhilePost => "while",
            NodeType::Until | NodeType::UntilPost => "until",
            _ => self.loc().keyword.map_or("", |k| self.range_source(k)),
        }
    }

    pub fn is_if(&self) -> bool {
        self.is(NodeType::If) && self.keyword() == "if"
    }

    pub fn is_unless(&self) -> bool {
        self.is(NodeType::If) && self.keyword() == "unless"
    }

    pub fn is_elsif(&self) -> bool {
        self.is(NodeType::If) && self.keyword() == "elsif"
    }

    pub fn is_ternary(&self) -> bool {
        self.is(NodeType::If) && self.loc().question.is_some()
    }

    /// `else?`
    pub fn has_else(&self) -> bool {
        self.loc().else_.is_some()
    }

    /// `then?`
    pub fn has_then(&self) -> bool {
        self.loc_is(self.loc().begin, "then")
    }

    /// `do?` (loops).
    pub fn has_do(&self) -> bool {
        self.loc_is(self.loc().begin, "do")
    }

    pub fn inverse_keyword(&self) -> &'static str {
        match self.ty() {
            NodeType::If => match self.keyword() {
                "if" => "unless",
                "unless" => "if",
                _ => "",
            },
            NodeType::While | NodeType::WhilePost => "until",
            NodeType::Until | NodeType::UntilPost => "while",
            _ => "",
        }
    }

    /// `modifier_form?`
    pub fn is_modifier_form(&self) -> bool {
        match self.ty() {
            NodeType::If => (self.is_if() || self.is_unless()) && self.loc().end.is_none(),
            NodeType::While | NodeType::Until | NodeType::WhilePost | NodeType::UntilPost => {
                self.loc().end.is_none()
            }
            _ => false,
        }
    }

    /// `node_parts` of an `if`: `[condition, true_branch, false_branch]`
    /// with the branches swapped for `unless`.
    fn if_parts(&self) -> (Option<Node<'a>>, Option<Node<'a>>, Option<Node<'a>>) {
        let (c, t, f) = (self.child(0), self.child(1), self.child(2));
        if self.is_unless() {
            (c, f, t)
        } else {
            (c, t, f)
        }
    }

    pub fn if_branch(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::If => self.if_parts().1,
            _ => None,
        }
    }

    pub fn else_branch(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::If => self.if_parts().2,
            NodeType::Case | NodeType::CaseMatch | NodeType::Rescue => self.child_from_end(1),
            _ => None,
        }
    }

    /// `elsif_conditional?`
    pub fn is_elsif_conditional(&self) -> bool {
        self.else_branch()
            .is_some_and(|e| e.is(NodeType::If) && e.is_elsif())
    }

    /// `nested_conditional?`
    pub fn is_nested_conditional(&self) -> bool {
        let (_, t, f) = self.if_parts();
        [t, f].into_iter().flatten().any(|branch| {
            branch
                .each_node()
                .any(|nested| nested.is(NodeType::If) && !nested.is_elsif())
        })
    }

    /// `branches` of `if`, `case`, `case_match`, `rescue`.
    pub fn branches(&self) -> Vec<Option<Node<'a>>> {
        match self.ty() {
            NodeType::If => {
                if self.is_ternary() {
                    vec![self.if_branch(), self.else_branch()]
                } else if !self.has_else() {
                    vec![self.if_branch()]
                } else {
                    let mut branches = vec![self.if_branch()];
                    if self.is_elsif_conditional() {
                        branches.extend(self.else_branch().unwrap().branches());
                    } else {
                        branches.push(self.else_branch());
                    }
                    branches
                }
            }
            NodeType::Case => {
                let mut bodies: Vec<Option<Node<'a>>> =
                    self.when_branches().iter().map(|w| w.body()).collect();
                if self.has_else() {
                    bodies.push(self.else_branch());
                }
                bodies
            }
            NodeType::CaseMatch => {
                let mut bodies: Vec<Option<Node<'a>>> =
                    self.when_branches().iter().map(|w| w.body()).collect();
                if self.has_else() {
                    let else_branch = self.else_branch();
                    if else_branch.is_some_and(|e| e.is(NodeType::EmptyElse)) {
                        bodies.push(None);
                    } else {
                        bodies.push(else_branch);
                    }
                }
                bodies
            }
            NodeType::Rescue => {
                let mut bodies: Vec<Option<Node<'a>>> =
                    self.resbody_branches().iter().map(|r| r.body()).collect();
                if self.has_else() {
                    bodies.push(self.else_branch());
                }
                bodies
            }
            _ => Vec::new(),
        }
    }

    /// `when_branches` / `in_pattern_branches`.
    pub fn when_branches(&self) -> Vec<Node<'a>> {
        let children = self.children();
        if children.len() < 2 {
            return Vec::new();
        }
        children[1..children.len() - 1]
            .iter()
            .filter_map(Child::as_node)
            .map(|id| self.node(id))
            .collect()
    }

    /// `conditions` of a `when` node; `conditions` of `and`/`or`.
    pub fn conditions(&self) -> Vec<Node<'a>> {
        match self.ty() {
            NodeType::When => {
                let children = self.children();
                children[..children.len().saturating_sub(1)]
                    .iter()
                    .filter_map(Child::as_node)
                    .map(|id| self.node(id))
                    .collect()
            }
            NodeType::And | NodeType::Or => {
                let mut out = Vec::new();
                for side in [self.child(0), self.child(1)].into_iter().flatten() {
                    let side = if side.is(NodeType::Begin) {
                        side.child(0).unwrap_or(side)
                    } else {
                        side
                    };
                    if side.is_operator_keyword() {
                        out.extend(side.conditions());
                    } else {
                        out.push(side);
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// `resbody_branches` of a `rescue` node.
    pub fn resbody_branches(&self) -> Vec<Node<'a>> {
        self.when_branches()
    }

    /// `exceptions` of a `resbody`.
    pub fn exceptions(&self) -> Vec<Node<'a>> {
        match self.child(0) {
            None => Vec::new(),
            Some(e) if e.is(NodeType::Array) => e.child_nodes().collect(),
            Some(e) => vec![e],
        }
    }

    /// `exception_variable` of a `resbody`.
    pub fn exception_variable(&self) -> Option<Node<'a>> {
        self.child(1)
    }

    /// `branch` of an `ensure` node.
    pub fn ensure_branch(&self) -> Option<Node<'a>> {
        self.child(1)
    }

    // ---- operators ---------------------------------------------------------------

    /// `lhs` of binary operators, assignments.
    pub fn lhs(&self) -> Option<Node<'a>> {
        use NodeType::*;
        match self.ty() {
            And | Or | OpAsgn | OrAsgn | AndAsgn | Masgn | Pair | Irange | Erange => self.child(0),
            _ => None,
        }
    }

    pub fn rhs(&self) -> Option<Node<'a>> {
        use NodeType::*;
        match self.ty() {
            And | Or | Pair | Irange | Erange => self.child(1),
            OpAsgn => self.child(2),
            OrAsgn | AndAsgn | Masgn => self.child(1),
            Lvasgn | Ivasgn | Cvasgn | Gvasgn => self.child(1),
            Casgn => self.child(2),
            _ => None,
        }
    }

    /// `expression` of assignments (`AsgnNode`, `CasgnNode`, `OpAsgnNode`).
    pub fn expression(&self) -> Option<Node<'a>> {
        self.rhs()
    }

    /// `operator` of `and`/`or` (the source of `loc.operator`).
    pub fn operator(&self) -> Option<&'a str> {
        self.loc().operator.map(|op| self.range_source(op))
    }

    pub fn is_logical_operator(&self) -> bool {
        matches!(self.operator(), Some("&&" | "||"))
    }

    pub fn is_semantic_operator(&self) -> bool {
        matches!(self.operator(), Some("and" | "or"))
    }

    // ---- hashes, pairs, arrays ------------------------------------------------

    pub fn pairs(&self) -> Vec<Node<'a>> {
        self.child_nodes()
            .filter(|c| c.is(NodeType::Pair))
            .collect()
    }

    pub fn key(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::Pair => self.child(0),
            NodeType::Kwsplat => Some(*self),
            _ => None,
        }
    }

    pub fn pair_value(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::Pair => self.child(1),
            NodeType::Kwsplat => Some(*self),
            _ => None,
        }
    }

    pub fn hash_keys(&self) -> Vec<Node<'a>> {
        self.pairs().iter().filter_map(|p| p.key()).collect()
    }

    pub fn hash_values(&self) -> Vec<Node<'a>> {
        self.pairs().iter().filter_map(|p| p.pair_value()).collect()
    }

    pub fn is_hash_rocket(&self) -> bool {
        self.is(NodeType::Pair) && self.loc_is(self.loc().operator, "=>")
    }

    pub fn is_colon(&self) -> bool {
        self.is(NodeType::Pair) && self.loc_is(self.loc().operator, ":")
    }

    /// `value_omission?` (`{x:}`).
    pub fn is_value_omission(&self) -> bool {
        self.is(NodeType::Pair) && self.source().ends_with(':')
    }

    pub fn is_square_brackets(&self) -> bool {
        self.loc_is(self.loc().begin, "[")
    }

    /// `percent_literal?` for arrays and strings.
    pub fn is_percent_literal(&self) -> bool {
        self.loc()
            .begin
            .is_some_and(|b| self.range_source(b).starts_with('%'))
    }

    /// `percent_literal?(:string)` / `(:symbol)` for arrays.
    pub fn is_percent_literal_of(&self, kind: &str) -> bool {
        let Some(begin) = self.loc().begin else {
            return false;
        };
        let source = self.range_source(begin);
        match kind {
            "string" => source.starts_with("%w") || source.starts_with("%W"),
            "symbol" => source.starts_with("%i") || source.starts_with("%I"),
            _ => false,
        }
    }

    pub fn is_bracketed(&self) -> bool {
        self.is_square_brackets() || self.is_percent_literal()
    }

    // ---- strings -------------------------------------------------------------------

    pub fn is_heredoc(&self) -> bool {
        self.loc().kind == MapKind::Heredoc
    }

    pub fn is_single_quoted(&self) -> bool {
        self.loc_is(self.loc().begin, "'")
    }

    pub fn is_double_quoted(&self) -> bool {
        self.loc_is(self.loc().begin, "\"")
    }

    pub fn is_character_literal(&self) -> bool {
        self.loc_is(self.loc().begin, "?")
    }

    // ---- constants ---------------------------------------------------------------

    pub fn namespace(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::Const | NodeType::Casgn => self.child(0),
            _ => None,
        }
    }

    pub fn short_name(&self) -> Option<&'a str> {
        match self.ty() {
            NodeType::Const | NodeType::Casgn => self.child_sym(1),
            _ => None,
        }
    }

    /// `const_name`: `Foo::Bar`.
    pub fn const_name(&self) -> Option<String> {
        if !matches!(self.ty(), NodeType::Const | NodeType::Casgn) {
            return None;
        }
        let short = self.short_name()?;
        match self.namespace() {
            Some(ns) if !ns.is(NodeType::Cbase) => {
                Some(format!("{}::{short}", ns.const_name().unwrap_or_default()))
            }
            _ => Some(short.to_owned()),
        }
    }

    /// `absolute?` (`::Foo`).
    pub fn is_absolute(&self) -> bool {
        let mut last = *self;
        loop {
            match last.child(0) {
                None => return false,
                Some(c) if c.is(NodeType::Cbase) => return true,
                Some(c) if c.is(NodeType::Const) => last = c,
                Some(_) => return false,
            }
        }
    }

    /// `global_const?(name)`: `(const {nil? cbase} name)`.
    pub fn is_global_const(&self, name: &str) -> bool {
        self.is(NodeType::Const)
            && self.namespace().is_none_or(|ns| ns.is(NodeType::Cbase))
            && self.short_name() == Some(name)
    }

    /// `identifier` of `class`/`module`/`sclass`.
    pub fn identifier(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::Class | NodeType::Module | NodeType::Sclass => self.child(0),
            _ => None,
        }
    }

    pub fn parent_class(&self) -> Option<Node<'a>> {
        match self.ty() {
            NodeType::Class => self.child(1),
            _ => None,
        }
    }

    /// `class_constructor?`
    pub fn is_class_constructor(&self) -> bool {
        let call = if self.is_any_block() {
            match self.send_node() {
                Some(s) => s,
                None => return false,
            }
        } else {
            *self
        };
        if !call.is_send() {
            return false;
        }
        let Some(receiver) = call.receiver() else {
            return false;
        };
        (call.is_method("new")
            && ["Class", "Module", "Struct"]
                .iter()
                .any(|n| receiver.is_global_const(n)))
            || (call.is_method("define") && receiver.is_global_const("Data"))
    }

    // ---- misc ------------------------------------------------------------------------

    /// `value_used?`
    pub fn is_value_used(&self) -> bool {
        let Some(parent) = self.parent() else {
            return false;
        };
        use NodeType::*;
        match parent.ty() {
            Array | Defined | Dstr | Dsym | Eflipflop | Erange | Float | Hash | Iflipflop
            | Irange | Not | Pair | Regexp | Str | Sym | When | Xstr => parent.is_value_used(),
            Begin | Kwbegin => {
                if self.sibling_index() == Some(parent.children().len() - 1) {
                    parent.is_value_used()
                } else {
                    false
                }
            }
            For => {
                if self.sibling_index() == Some(2) {
                    parent.is_value_used()
                } else {
                    true
                }
            }
            Case | If => self.sibling_index() == Some(0) || parent.is_value_used(),
            While | Until | WhilePost | UntilPost => self.sibling_index() == Some(0),
            _ => true,
        }
    }

    /// `pure?`
    pub fn is_pure(&self) -> bool {
        use NodeType::*;
        match self.ty() {
            File | Line | Const | Cvar | Defined | False | Float | Gvar | Int | Ivar | Lvar
            | Nil | Str | Sym | True | Regopt => true,
            And | Array | Begin | Case | Dstr | Dsym | Eflipflop | Ensure | Erange | For | Hash
            | If | Iflipflop | Irange | Kwbegin | Not | Or | Pair | Regexp | Until | UntilPost
            | When | While | WhilePost => self.child_nodes().all(|c| c.is_pure()),
            _ => false,
        }
    }

    /// `guard_clause?`
    pub fn is_guard_clause(&self) -> bool {
        let node = if self.is_operator_keyword() {
            match self.rhs() {
                Some(r) => r,
                None => return false,
            }
        } else {
            *self
        };
        let matched = (node.is_send()
            && node.receiver().is_none()
            && matches!(node.method_name(), Some("raise" | "fail")))
            || matches!(
                node.ty(),
                NodeType::Return | NodeType::Break | NodeType::Next
            );
        matched && node.is_single_line()
    }

    /// `proc?`
    pub fn is_proc(&self) -> bool {
        if self.is(NodeType::Block) {
            let Some(send) = self.send_node() else {
                return false;
            };
            return (send.is_send()
                && send.receiver().is_none()
                && send.is_method("proc")
                && send.children().len() == 2)
                || (send.is_send()
                    && send.is_method("new")
                    && send.children().len() == 2
                    && send.receiver().is_some_and(|r| r.is_global_const("Proc")));
        }
        self.is_send()
            && self.is_method("new")
            && self.children().len() == 2
            && self.receiver().is_some_and(|r| r.is_global_const("Proc"))
    }

    /// `lambda_or_proc?`
    pub fn is_lambda_or_proc(&self) -> bool {
        (self.is_any_block()
            && self
                .send_node()
                .is_some_and(|s| s.is_send() && s.receiver().is_none() && s.is_method("lambda")))
            || self.is_proc()
    }

    /// `parent_module_name`
    pub fn parent_module_name(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        for ancestor in self.ancestors() {
            use NodeType::*;
            match ancestor.ty() {
                Class | Module | Casgn => {
                    if let Some(name) = ancestor.defined_module_name() {
                        parts.push(name);
                    }
                }
                Sclass => {
                    let subject = ancestor.child(0)?;
                    if subject.is(Const) {
                        parts.push(format!("#<Class:{}>", subject.const_name()?));
                    } else if subject.is(SelfNode) {
                        parts.push(format!("#<Class:{}>", ancestor.parent_module_name()?));
                    } else {
                        return None;
                    }
                }
                Block => {
                    if ancestor.is_method("class_eval") {
                        let receiver = ancestor.receiver()?;
                        if !receiver.is(Const) {
                            return None;
                        }
                        parts.push(receiver.const_name()?);
                    } else if !ancestor.is_new_class_or_module_block() {
                        return None;
                    }
                }
                _ => {}
            }
        }
        parts.reverse();
        let result = parts.join("::");
        Some(if result.is_empty() {
            "Object".to_owned()
        } else {
            result
        })
    }

    fn is_new_class_or_module_block(&self) -> bool {
        let Some(parent) = self.parent() else {
            return false;
        };
        if !parent.is(NodeType::Casgn) || parent.child(2) != Some(*self) {
            return false;
        }
        let Some(send) = self.send_node() else {
            return false;
        };
        send.is_send()
            && send.is_method("new")
            && send.children().len() == 2
            && send.receiver().is_some_and(|r| {
                r.is(NodeType::Const) && matches!(r.short_name(), Some("Class" | "Module"))
            })
    }

    /// `defined_module_name`
    pub fn defined_module_name(&self) -> Option<String> {
        use NodeType::*;
        match self.ty() {
            Class | Module => {
                let c = self.child(0)?;
                if c.is(Const) { c.const_name() } else { None }
            }
            Casgn => {
                let value = self.child(2)?;
                let call = if value.is(Block) {
                    value.send_node()?
                } else {
                    value
                };
                let ok = call.is_send()
                    && call.is_method("new")
                    && call
                        .receiver()
                        .is_some_and(|r| r.is_global_const("Class") || r.is_global_const("Module"));
                if ok { self.const_name() } else { None }
            }
            _ => None,
        }
    }
}
