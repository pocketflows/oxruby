//! Port of `Parser::Builders::Default` (parser gem 3.3) and
//! `Prism::Translation::Parser::Builder`, limited to what the Prism compiler
//! calls. RuboCop's builder keeps the legacy `emit_*` settings except
//! `emit_forward_arg` and `emit_match_pattern`, so `-> {}` is
//! `(send nil :lambda)`, `a[1]` is `(send a :[] 1)` and keyword arguments are
//! a plain `hash`.
//!
//! Diagnostics the builder reports are left out: oxcop only builds the AST
//! for sources Prism accepts.

use crate::ast::{Child, Loc, MapKind, NodeId, NodeType};
use crate::processed_source::Range;

/// A token as the builder receives it: its value and location.
#[derive(Debug, Clone)]
pub struct Tok {
    pub value: String,
    pub range: Range,
}

impl Tok {
    pub fn new(value: impl Into<String>, range: Range) -> Self {
        Self {
            value: value.into(),
            range,
        }
    }
}

pub type T = Option<Tok>;

#[derive(Debug, Clone)]
pub struct BuildNode {
    pub ty: NodeType,
    pub children: Vec<Child>,
    pub loc: Loc,
}

pub fn join(a: Range, b: Range) -> Range {
    Range::new(a.begin_pos.min(b.begin_pos), a.end_pos.max(b.end_pos))
}

fn loc(token: &T) -> Option<Range> {
    token.as_ref().map(|t| t.range)
}

fn value(token: &T) -> &str {
    token.as_ref().map_or("", |t| t.value.as_str())
}

pub struct Builder<'s> {
    pub nodes: Vec<BuildNode>,
    pub source: &'s str,
    file_name: String,
    line_starts: Vec<usize>,
}

impl<'s> Builder<'s> {
    pub fn new(source: &'s str, file_name: &str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        Self {
            nodes: Vec::new(),
            source,
            file_name: file_name.to_owned(),
            line_starts,
        }
    }

    // ---- arena helpers -------------------------------------------------

    pub fn n(&mut self, ty: NodeType, children: Vec<Child>, loc: Loc) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(BuildNode { ty, children, loc });
        id
    }

    fn n0(&mut self, ty: NodeType, loc: Loc) -> NodeId {
        self.n(ty, Vec::new(), loc)
    }

    pub fn ty(&self, id: NodeId) -> NodeType {
        self.nodes[id as usize].ty
    }

    pub fn node(&self, id: NodeId) -> &BuildNode {
        &self.nodes[id as usize]
    }

    pub fn node_mut(&mut self, id: NodeId) -> &mut BuildNode {
        &mut self.nodes[id as usize]
    }

    pub fn expr(&self, id: NodeId) -> Range {
        self.nodes[id as usize]
            .loc
            .expression
            .expect("node has an expression")
    }

    fn opt_expr(&self, id: Option<NodeId>) -> Option<Range> {
        id.and_then(|id| self.nodes[id as usize].loc.expression)
    }

    fn join_exprs(&self, left: NodeId, right: NodeId) -> Range {
        join(self.expr(left), self.expr(right))
    }

    fn first_child_node(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).children.first().and_then(Child::as_node)
    }

    fn nodes_children(ids: &[Option<NodeId>]) -> Vec<Child> {
        ids.iter()
            .map(|id| id.map_or(Child::Nil, Child::Node))
            .collect()
    }

    fn some_children(ids: &[NodeId]) -> Vec<Child> {
        ids.iter().map(|&id| Child::Node(id)).collect()
    }

    // ---- maps ------------------------------------------------------------

    fn token_map(&self, token: &T) -> Loc {
        Loc::new(MapKind::Map, loc(token))
    }

    fn expr_map(&self, range: Range) -> Loc {
        Loc::new(MapKind::Map, Some(range))
    }

    fn prefix_string_map(&self, symbol: &T) -> Loc {
        let r = loc(symbol).unwrap();
        Loc {
            begin: Some(Range::new(r.begin_pos, r.begin_pos + 1)),
            ..Loc::new(MapKind::Collection, Some(r))
        }
    }

    fn unquoted_map(&self, token: &T) -> Loc {
        Loc::new(MapKind::Collection, loc(token))
    }

    /// `parts` as the first and last part's expressions, if any.
    fn collection_map_ranges(&self, begin_t: &T, parts: Option<(Range, Range)>, end_t: &T) -> Loc {
        let expr_l = if begin_t.is_none() || end_t.is_none() {
            if let Some((first, last)) = parts {
                Some(join(first, last))
            } else if begin_t.is_some() {
                loc(begin_t)
            } else {
                loc(end_t)
            }
        } else {
            Some(join(loc(begin_t).unwrap(), loc(end_t).unwrap()))
        };
        Loc {
            begin: loc(begin_t),
            end: loc(end_t),
            ..Loc::new(MapKind::Collection, expr_l)
        }
    }

    fn parts_range(&self, parts: &[NodeId]) -> Option<(Range, Range)> {
        match (parts.first(), parts.last()) {
            (Some(&first), Some(&last)) => Some((self.expr(first), self.expr(last))),
            _ => None,
        }
    }

    fn collection_map(&self, begin_t: &T, parts: &[NodeId], end_t: &T) -> Loc {
        self.collection_map_ranges(begin_t, self.parts_range(parts), end_t)
    }

    fn string_map(&self, begin_t: &T, parts: &[NodeId], end_t: &T) -> Loc {
        if begin_t.as_ref().is_some_and(|t| t.value.starts_with("<<")) {
            let expr_l = match self.parts_range(parts) {
                Some((first, last)) => join(first, last),
                None => {
                    let end = loc(end_t).unwrap();
                    Range::new(end.begin_pos, end.begin_pos)
                }
            };
            Loc {
                heredoc_body: Some(expr_l),
                heredoc_end: loc(end_t),
                ..Loc::new(MapKind::Heredoc, loc(begin_t))
            }
        } else {
            self.collection_map(begin_t, parts, end_t)
        }
    }

    fn regexp_map(&self, begin_t: &T, end_t: &T, options: NodeId) -> Loc {
        Loc {
            begin: loc(begin_t),
            end: loc(end_t),
            ..Loc::new(
                MapKind::Collection,
                Some(join(loc(begin_t).unwrap(), self.expr(options))),
            )
        }
    }

    fn constant_map(&self, scope: Option<NodeId>, colon2_t: &T, name_t: &T) -> Loc {
        let expr_l = match scope {
            None => loc(name_t).unwrap(),
            Some(scope) => join(self.expr(scope), loc(name_t).unwrap()),
        };
        Loc {
            double_colon: loc(colon2_t),
            name: loc(name_t),
            ..Loc::new(MapKind::Constant, Some(expr_l))
        }
    }

    fn variable_map(&self, name_t: &T) -> Loc {
        Loc {
            name: loc(name_t),
            ..Loc::new(MapKind::Variable, loc(name_t))
        }
    }

    fn binary_op_map(&self, left: NodeId, op_t: &T, right: NodeId) -> Loc {
        Loc {
            operator: loc(op_t),
            ..Loc::new(MapKind::Operator, Some(self.join_exprs(left, right)))
        }
    }

    fn unary_op_map(&self, op_t: &T, arg: Option<NodeId>) -> Loc {
        let expr_l = match arg {
            None => loc(op_t).unwrap(),
            Some(arg) => join(loc(op_t).unwrap(), self.expr(arg)),
        };
        Loc {
            operator: loc(op_t),
            ..Loc::new(MapKind::Operator, Some(expr_l))
        }
    }

    fn range_map(&self, start: Option<NodeId>, op_t: &T, end: Option<NodeId>) -> Loc {
        let op = loc(op_t).unwrap();
        let expr_l = match (start, end) {
            (Some(s), Some(e)) => self.join_exprs(s, e),
            (Some(s), None) => join(self.expr(s), op),
            (None, Some(e)) => join(op, self.expr(e)),
            (None, None) => op,
        };
        Loc {
            operator: Some(op),
            ..Loc::new(MapKind::Operator, Some(expr_l))
        }
    }

    fn arg_prefix_map(&self, op_t: &T, name_t: &T) -> Loc {
        let expr_l = match loc(name_t) {
            None => loc(op_t).unwrap(),
            Some(name) => join(loc(op_t).unwrap(), name),
        };
        Loc {
            name: loc(name_t),
            ..Loc::new(MapKind::Variable, Some(expr_l))
        }
    }

    fn kwarg_map(&self, name_t: &T, value: Option<NodeId>) -> Loc {
        let label = loc(name_t).unwrap();
        let name_range = Range::new(label.begin_pos, label.end_pos - 1);
        let expr_l = match value {
            Some(value) => join(label, self.expr(value)),
            None => label,
        };
        Loc {
            name: Some(name_range),
            ..Loc::new(MapKind::Variable, Some(expr_l))
        }
    }

    fn module_definition_map(
        &self,
        keyword_t: &T,
        name: Option<NodeId>,
        operator_t: &T,
        end_t: &T,
    ) -> Loc {
        Loc {
            keyword: loc(keyword_t),
            operator: loc(operator_t),
            name: self.opt_expr(name),
            end: loc(end_t),
            ..Loc::new(
                MapKind::Definition,
                Some(join(loc(keyword_t).unwrap(), loc(end_t).unwrap())),
            )
        }
    }

    fn definition_map(&self, keyword_t: &T, operator_t: &T, name_t: &T, end_t: &T) -> Loc {
        Loc {
            keyword: loc(keyword_t),
            operator: loc(operator_t),
            name: loc(name_t),
            end: loc(end_t),
            ..Loc::new(
                MapKind::MethodDefinition,
                Some(join(loc(keyword_t).unwrap(), loc(end_t).unwrap())),
            )
        }
    }

    fn endless_definition_map(
        &self,
        keyword_t: &T,
        operator_t: &T,
        name_t: &T,
        assignment_t: &T,
        body: Option<NodeId>,
    ) -> Loc {
        let body_l = self.opt_expr(body).unwrap();
        Loc {
            keyword: loc(keyword_t),
            operator: loc(operator_t),
            name: loc(name_t),
            assignment: loc(assignment_t),
            ..Loc::new(
                MapKind::MethodDefinition,
                Some(join(loc(keyword_t).unwrap(), body_l)),
            )
        }
    }

    fn send_map(
        &self,
        receiver: Option<NodeId>,
        dot_t: &T,
        selector_t: &T,
        begin_t: &T,
        args: &[NodeId],
        end_t: &T,
    ) -> Loc {
        let begin_l = match receiver {
            Some(receiver) => Some(self.expr(receiver)),
            None => loc(selector_t),
        };
        let end_l = if end_t.is_some() {
            loc(end_t)
        } else if let Some(&last) = args.last() {
            Some(self.expr(last))
        } else {
            loc(selector_t)
        };
        let expr = match (begin_l, end_l) {
            (Some(b), Some(e)) => Some(join(b, e)),
            (b, e) => b.or(e),
        };
        Loc {
            dot: loc(dot_t),
            selector: loc(selector_t),
            begin: loc(begin_t),
            end: loc(end_t),
            ..Loc::new(MapKind::Send, expr)
        }
    }

    fn send_binary_op_map(&self, lhs: NodeId, selector_t: &T, rhs: NodeId) -> Loc {
        Loc {
            selector: loc(selector_t),
            ..Loc::new(MapKind::Send, Some(self.join_exprs(lhs, rhs)))
        }
    }

    fn send_unary_op_map(&self, selector_t: &T, arg: Option<NodeId>) -> Loc {
        let expr_l = match arg {
            None => loc(selector_t).unwrap(),
            Some(arg) => join(loc(selector_t).unwrap(), self.expr(arg)),
        };
        Loc {
            selector: loc(selector_t),
            ..Loc::new(MapKind::Send, Some(expr_l))
        }
    }

    fn send_index_map(&self, receiver: NodeId, lbrack_t: &T, rbrack_t: &T) -> Loc {
        Loc {
            selector: Some(join(loc(lbrack_t).unwrap(), loc(rbrack_t).unwrap())),
            ..Loc::new(
                MapKind::Send,
                Some(join(self.expr(receiver), loc(rbrack_t).unwrap())),
            )
        }
    }

    fn block_map(&self, receiver_l: Range, begin_t: &T, end_t: &T) -> Loc {
        Loc {
            begin: loc(begin_t),
            end: loc(end_t),
            ..Loc::new(
                MapKind::Collection,
                Some(join(receiver_l, loc(end_t).unwrap())),
            )
        }
    }

    fn keyword_map(&self, keyword_t: &T, begin_t: &T, args: &[Option<NodeId>], end_t: &T) -> Loc {
        let end_l = if end_t.is_some() {
            loc(end_t).unwrap()
        } else if let Some(Some(last)) = args.last() {
            self.expr(*last)
        } else if args.len() > 1
            && let Some(prev) = args[args.len() - 2]
        {
            self.expr(prev)
        } else {
            loc(keyword_t).unwrap()
        };
        Loc {
            keyword: loc(keyword_t),
            begin: loc(begin_t),
            end: loc(end_t),
            ..Loc::new(MapKind::Keyword, Some(join(loc(keyword_t).unwrap(), end_l)))
        }
    }

    fn keyword_mod_map(&self, pre: NodeId, keyword_t: &T, post: NodeId) -> Loc {
        Loc {
            keyword: loc(keyword_t),
            ..Loc::new(MapKind::Keyword, Some(self.join_exprs(pre, post)))
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn condition_map(
        &self,
        keyword_t: &T,
        cond: Option<NodeId>,
        begin_t: &T,
        body: Option<NodeId>,
        else_t: &T,
        else_e: Option<NodeId>,
        end_t: &T,
    ) -> Loc {
        let end_l = if end_t.is_some() {
            loc(end_t)
        } else if let Some(e) = self.opt_expr(else_e) {
            Some(e)
        } else if else_t.is_some() {
            loc(else_t)
        } else if let Some(b) = self.opt_expr(body) {
            Some(b)
        } else if begin_t.is_some() {
            loc(begin_t)
        } else {
            self.opt_expr(cond)
        };
        let kw = loc(keyword_t).unwrap();
        Loc {
            keyword: loc(keyword_t),
            begin: loc(begin_t),
            else_: loc(else_t),
            end: loc(end_t),
            ..Loc::new(MapKind::Condition, Some(end_l.map_or(kw, |e| join(kw, e))))
        }
    }

    fn ternary_map(&self, begin_e: NodeId, question_t: &T, colon_t: &T, end_e: NodeId) -> Loc {
        Loc {
            question: loc(question_t),
            colon: loc(colon_t),
            ..Loc::new(MapKind::Ternary, Some(self.join_exprs(begin_e, end_e)))
        }
    }

    fn for_map(&self, keyword_t: &T, in_t: &T, begin_t: &T, end_t: &T) -> Loc {
        Loc {
            keyword: loc(keyword_t),
            in_: loc(in_t),
            begin: loc(begin_t),
            end: loc(end_t),
            ..Loc::new(
                MapKind::For,
                Some(join(loc(keyword_t).unwrap(), loc(end_t).unwrap())),
            )
        }
    }

    fn rescue_body_map(
        &self,
        keyword_t: &T,
        exc_list: Option<NodeId>,
        assoc_t: &T,
        exc_var: Option<NodeId>,
        then_t: &T,
        compstmt: Option<NodeId>,
    ) -> Loc {
        let end_l = self
            .opt_expr(compstmt)
            .or_else(|| loc(then_t))
            .or_else(|| self.opt_expr(exc_var))
            .or_else(|| self.opt_expr(exc_list))
            .or_else(|| loc(keyword_t))
            .unwrap();
        Loc {
            keyword: loc(keyword_t),
            assoc: loc(assoc_t),
            begin: loc(then_t),
            ..Loc::new(
                MapKind::RescueBody,
                Some(join(loc(keyword_t).unwrap(), end_l)),
            )
        }
    }

    fn eh_keyword_map(
        &self,
        compstmt: Option<NodeId>,
        keyword_t: &T,
        body_es: &[Option<NodeId>],
        else_t: &T,
        else_e: Option<NodeId>,
    ) -> Loc {
        let begin_l = match compstmt {
            None => match keyword_t {
                None => self.opt_expr(body_es[0]),
                Some(_) => loc(keyword_t),
            },
            Some(c) => Some(self.expr(c)),
        };
        let end_l = if else_t.is_some() {
            match else_e {
                None => loc(else_t),
                Some(e) => Some(self.expr(e)),
            }
        } else if let Some(Some(last)) = body_es.last() {
            Some(self.expr(*last))
        } else {
            loc(keyword_t)
        };
        let expr = match (begin_l, end_l) {
            (Some(b), Some(e)) => Some(join(b, e)),
            (b, e) => b.or(e),
        };
        Loc {
            keyword: loc(keyword_t),
            else_: loc(else_t),
            ..Loc::new(MapKind::Condition, expr)
        }
    }

    fn guard_map(&self, keyword_t: &T, body: NodeId) -> Loc {
        Loc {
            keyword: loc(keyword_t),
            ..Loc::new(
                MapKind::Keyword,
                Some(join(loc(keyword_t).unwrap(), self.expr(body))),
            )
        }
    }

    // ---- literals ----------------------------------------------------------

    pub fn nil(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::Nil, l)
    }

    pub fn true_(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::True, l)
    }

    pub fn false_(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::False, l)
    }

    /// `integer`, `float`, `rational` and `complex`.
    pub fn numeric(&mut self, ty: NodeType, value: Child, range: Range) -> NodeId {
        self.n(ty, vec![value], Loc::new(MapKind::Operator, Some(range)))
    }

    /// The value is not negated: the Prism compiler passes the signed value
    /// and the sign as a `Symbol`, which never equals `'-'`.
    pub fn unary_num(&mut self, unary_t: T, numeric: NodeId) -> NodeId {
        let operator_loc = loc(&unary_t).unwrap();
        let expr = join(operator_loc, self.expr(numeric));
        let node = self.node_mut(numeric);
        node.loc = Loc {
            operator: Some(operator_loc),
            ..Loc::new(MapKind::Operator, Some(expr))
        };
        numeric
    }

    /// `string_internal` for a value given as raw (unescaped) bytes.
    pub fn string_internal_bytes(&mut self, value: &[u8], range: Range) -> NodeId {
        let l = Loc::new(MapKind::Collection, Some(range));
        self.n(NodeType::Str, vec![Child::string(value.to_vec())], l)
    }

    /// `character` for a value given as raw bytes.
    pub fn character_bytes(&mut self, value: &[u8], range: Range) -> NodeId {
        let l = Loc {
            begin: Some(Range::new(range.begin_pos, range.begin_pos + 1)),
            ..Loc::new(MapKind::Collection, Some(range))
        };
        self.n(NodeType::Str, vec![Child::string(value.to_vec())], l)
    }

    fn collapse_string_parts(&self, parts: &[NodeId]) -> bool {
        parts.len() == 1 && matches!(self.ty(parts[0]), NodeType::Str | NodeType::Dstr)
    }

    pub fn string_compose(&mut self, begin_t: T, parts: Vec<NodeId>, end_t: T) -> NodeId {
        if self.collapse_string_parts(&parts) {
            if begin_t.is_none() && end_t.is_none() {
                parts[0]
            } else {
                let children = self.node(parts[0]).children.clone();
                let l = self.string_map(&begin_t, &parts, &end_t);
                self.n(NodeType::Str, children, l)
            }
        } else {
            let l = self.string_map(&begin_t, &parts, &end_t);
            self.n(NodeType::Dstr, Self::some_children(&parts), l)
        }
    }

    pub fn file(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::File, l)
    }

    pub fn line(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::Line, l)
    }

    pub fn encoding(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::Encoding, l)
    }

    pub fn symbol(&mut self, t: T) -> NodeId {
        let l = self.prefix_string_map(&t);
        let v = value(&t).to_owned();
        self.n(NodeType::Sym, vec![Child::Sym(v)], l)
    }

    pub fn symbol_internal(&mut self, t: T) -> NodeId {
        let l = self.unquoted_map(&t);
        let v = value(&t).to_owned();
        self.n(NodeType::Sym, vec![Child::Sym(v)], l)
    }

    pub fn symbol_compose(&mut self, begin_t: T, parts: Vec<NodeId>, end_t: T) -> NodeId {
        if self.collapse_string_parts(&parts) {
            let str_node = self.node(parts[0]);
            let value = match str_node.children.first().and_then(Child::str_bytes) {
                Some(s) => String::from_utf8_lossy(s).into_owned(),
                None => String::new(),
            };
            let e = str_node.loc.expression.unwrap();
            let l = self.collection_map_ranges(&begin_t, Some((e, e)), &end_t);
            self.n(NodeType::Sym, vec![Child::Sym(value)], l)
        } else {
            let l = self.collection_map(&begin_t, &parts, &end_t);
            self.n(NodeType::Dsym, Self::some_children(&parts), l)
        }
    }

    pub fn xstring_compose(&mut self, begin_t: T, parts: Vec<NodeId>, end_t: T) -> NodeId {
        let l = self.string_map(&begin_t, &parts, &end_t);
        self.n(NodeType::Xstr, Self::some_children(&parts), l)
    }

    pub fn regexp_options(&mut self, t: T) -> NodeId {
        let mut options: Vec<char> = value(&t).chars().collect();
        options.sort_unstable();
        options.dedup();
        let l = self.token_map(&t);
        let children = options
            .into_iter()
            .map(|c| Child::Sym(c.to_string()))
            .collect();
        self.n(NodeType::Regopt, children, l)
    }

    pub fn regexp_compose(
        &mut self,
        begin_t: T,
        mut parts: Vec<NodeId>,
        end_t: T,
        options: NodeId,
    ) -> NodeId {
        let l = self.regexp_map(&begin_t, &end_t, options);
        parts.push(options);
        self.n(NodeType::Regexp, Self::some_children(&parts), l)
    }

    pub fn array(&mut self, begin_t: T, elements: Vec<NodeId>, end_t: T) -> NodeId {
        let l = self.collection_map(&begin_t, &elements, &end_t);
        self.n(NodeType::Array, Self::some_children(&elements), l)
    }

    pub fn splat(&mut self, star_t: T, arg: Option<NodeId>) -> NodeId {
        let l = self.unary_op_map(&star_t, arg);
        match arg {
            None => self.n0(NodeType::Splat, l),
            Some(arg) => self.n(NodeType::Splat, vec![Child::Node(arg)], l),
        }
    }

    pub fn pair(&mut self, key: NodeId, assoc_t: T, value: NodeId) -> NodeId {
        let l = self.binary_op_map(key, &assoc_t, value);
        self.n(
            NodeType::Pair,
            vec![Child::Node(key), Child::Node(value)],
            l,
        )
    }

    pub fn pair_keyword(&mut self, key_t: T, value_n: NodeId) -> NodeId {
        let key_range = loc(&key_t).unwrap();
        let key_l = Range::new(key_range.begin_pos, key_range.end_pos - 1);
        let colon_l = Range::new(key_range.end_pos - 1, key_range.end_pos);
        let key_map = Loc::new(MapKind::Collection, Some(key_l));
        let pair_map = Loc {
            operator: Some(colon_l),
            ..Loc::new(MapKind::Operator, Some(join(key_range, self.expr(value_n))))
        };
        let key = self.n(
            NodeType::Sym,
            vec![Child::Sym(value(&key_t).to_owned())],
            key_map,
        );
        self.n(
            NodeType::Pair,
            vec![Child::Node(key), Child::Node(value_n)],
            pair_map,
        )
    }

    pub fn pair_quoted(
        &mut self,
        begin_t: T,
        parts: Vec<NodeId>,
        end_t: T,
        value_n: NodeId,
    ) -> NodeId {
        let end_l = loc(&end_t).unwrap();
        let quote_l = Range::new(end_l.end_pos - 2, end_l.end_pos - 1);
        let colon_l = Range::new(end_l.end_pos - 1, end_l.end_pos);
        let end_t = Some(Tok::new(value(&end_t), quote_l));
        let pair_map = Loc {
            operator: Some(colon_l),
            ..Loc::new(
                MapKind::Operator,
                Some(join(loc(&begin_t).unwrap(), self.expr(value_n))),
            )
        };
        let key = self.symbol_compose(begin_t, parts, end_t);
        self.n(
            NodeType::Pair,
            vec![Child::Node(key), Child::Node(value_n)],
            pair_map,
        )
    }

    pub fn kwsplat(&mut self, dstar_t: T, arg: Option<NodeId>) -> NodeId {
        let l = self.unary_op_map(&dstar_t, arg);
        self.n(NodeType::Kwsplat, Self::nodes_children(&[arg]), l)
    }

    pub fn associate(&mut self, begin_t: T, pairs: Vec<NodeId>, end_t: T) -> NodeId {
        let l = self.collection_map(&begin_t, &pairs, &end_t);
        self.n(NodeType::Hash, Self::some_children(&pairs), l)
    }

    pub fn range(
        &mut self,
        ty: NodeType,
        lhs: Option<NodeId>,
        op_t: T,
        rhs: Option<NodeId>,
    ) -> NodeId {
        let l = self.range_map(lhs, &op_t, rhs);
        self.n(ty, Self::nodes_children(&[lhs, rhs]), l)
    }

    // ---- access ------------------------------------------------------------

    pub fn self_(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        self.n0(NodeType::SelfNode, l)
    }

    pub fn ident(&mut self, t: T) -> NodeId {
        self.variable(NodeType::Ident, t)
    }

    /// `ident`, `ivar`, `gvar` and `cvar`.
    pub fn variable(&mut self, ty: NodeType, t: T) -> NodeId {
        let l = self.variable_map(&t);
        let v = value(&t).to_owned();
        self.n(ty, vec![Child::Sym(v)], l)
    }

    pub fn back_ref(&mut self, t: T) -> NodeId {
        let l = self.token_map(&t);
        let v = value(&t).to_owned();
        self.n(NodeType::BackRef, vec![Child::Sym(v)], l)
    }

    pub fn nth_ref(&mut self, number: i64, range: Range) -> NodeId {
        self.n(
            NodeType::NthRef,
            vec![Child::Int(number)],
            Loc::new(MapKind::Map, Some(range)),
        )
    }

    /// Port of `accessible` for `__FILE__`, `__LINE__` and `__ENCODING__`.
    pub fn accessible(&mut self, node: NodeId) -> NodeId {
        let l = self.node(node).loc.clone();
        match self.ty(node) {
            NodeType::File => {
                let name = self.file_name.clone();
                self.n(NodeType::Str, vec![Child::Str(name)], l)
            }
            NodeType::Line => {
                let begin = l.expression.unwrap().begin_pos;
                let line = match self.line_starts.binary_search(&begin) {
                    Ok(i) => i + 1,
                    Err(i) => i,
                };
                self.n(NodeType::Int, vec![Child::Int(line as i64)], l)
            }
            NodeType::Encoding => {
                let inner = self.n(
                    NodeType::Const,
                    vec![Child::Nil, Child::Sym("Encoding".to_owned())],
                    Loc::default(),
                );
                self.n(
                    NodeType::Const,
                    vec![Child::Node(inner), Child::Sym("UTF_8".to_owned())],
                    l,
                )
            }
            _ => node,
        }
    }

    pub fn const_(&mut self, name_t: T) -> NodeId {
        let l = self.constant_map(None, &None, &name_t);
        let v = value(&name_t).to_owned();
        self.n(NodeType::Const, vec![Child::Nil, Child::Sym(v)], l)
    }

    pub fn const_global(&mut self, t_colon3: T, name_t: T) -> NodeId {
        let cl = self.token_map(&t_colon3);
        let cbase = self.n0(NodeType::Cbase, cl);
        let l = self.constant_map(Some(cbase), &t_colon3, &name_t);
        let v = value(&name_t).to_owned();
        self.n(NodeType::Const, vec![Child::Node(cbase), Child::Sym(v)], l)
    }

    pub fn const_fetch(&mut self, scope: NodeId, t_colon2: T, name_t: T) -> NodeId {
        let l = self.constant_map(Some(scope), &t_colon2, &name_t);
        let v = value(&name_t).to_owned();
        self.n(NodeType::Const, vec![Child::Node(scope), Child::Sym(v)], l)
    }

    // ---- assignment ----------------------------------------------------------

    pub fn assignable(&mut self, node: NodeId) -> NodeId {
        let ty = match self.ty(node) {
            NodeType::Cvar => NodeType::Cvasgn,
            NodeType::Ivar => NodeType::Ivasgn,
            NodeType::Gvar => NodeType::Gvasgn,
            NodeType::Const => NodeType::Casgn,
            NodeType::Ident => NodeType::Lvasgn,
            other => other,
        };
        self.node_mut(node).ty = ty;
        node
    }

    pub fn assign(&mut self, lhs: NodeId, eql_t: T, rhs: NodeId) -> NodeId {
        let expr = self.join_exprs(lhs, rhs);
        let node = self.node_mut(lhs);
        node.children.push(Child::Node(rhs));
        node.loc.operator = loc(&eql_t);
        node.loc.expression = Some(expr);
        lhs
    }

    pub fn op_assign(&mut self, lhs: NodeId, op_t: T, rhs: NodeId) -> NodeId {
        let mut source_map = self.node(lhs).loc.clone();
        source_map.operator = loc(&op_t);
        source_map.expression = Some(self.join_exprs(lhs, rhs));
        if self.ty(lhs) == NodeType::Index {
            self.node_mut(lhs).ty = NodeType::Indexasgn;
        }
        match value(&op_t) {
            "&&" => self.n(
                NodeType::AndAsgn,
                vec![Child::Node(lhs), Child::Node(rhs)],
                source_map,
            ),
            "||" => self.n(
                NodeType::OrAsgn,
                vec![Child::Node(lhs), Child::Node(rhs)],
                source_map,
            ),
            op => {
                let op = op.to_owned();
                self.n(
                    NodeType::OpAsgn,
                    vec![Child::Node(lhs), Child::Sym(op), Child::Node(rhs)],
                    source_map,
                )
            }
        }
    }

    pub fn multi_lhs(&mut self, begin_t: T, items: Vec<NodeId>, end_t: T) -> NodeId {
        let l = self.collection_map(&begin_t, &items, &end_t);
        self.n(NodeType::Mlhs, Self::some_children(&items), l)
    }

    pub fn multi_assign(&mut self, lhs: NodeId, eql_t: T, rhs: NodeId) -> NodeId {
        let l = self.binary_op_map(lhs, &eql_t, rhs);
        self.n(NodeType::Masgn, vec![Child::Node(lhs), Child::Node(rhs)], l)
    }

    // ---- class and module definition -------------------------------------

    pub fn def_class(
        &mut self,
        class_t: T,
        name: NodeId,
        lt_t: T,
        superclass: Option<NodeId>,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.module_definition_map(&class_t, Some(name), &lt_t, &end_t);
        self.n(
            NodeType::Class,
            Self::nodes_children(&[Some(name), superclass, body]),
            l,
        )
    }

    pub fn def_sclass(
        &mut self,
        class_t: T,
        lshft_t: T,
        expr: NodeId,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.module_definition_map(&class_t, None, &lshft_t, &end_t);
        self.n(
            NodeType::Sclass,
            Self::nodes_children(&[Some(expr), body]),
            l,
        )
    }

    pub fn def_module(
        &mut self,
        module_t: T,
        name: NodeId,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.module_definition_map(&module_t, Some(name), &None, &end_t);
        self.n(
            NodeType::Module,
            Self::nodes_children(&[Some(name), body]),
            l,
        )
    }

    // ---- method (un)definition -----------------------------------------------

    pub fn def_method(
        &mut self,
        def_t: T,
        name_t: T,
        args: NodeId,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.definition_map(&def_t, &None, &name_t, &end_t);
        let mut children = vec![Child::Sym(value(&name_t).to_owned())];
        children.extend(Self::nodes_children(&[Some(args), body]));
        self.n(NodeType::Def, children, l)
    }

    pub fn def_endless_method(
        &mut self,
        def_t: T,
        name_t: T,
        args: NodeId,
        assignment_t: T,
        body: Option<NodeId>,
    ) -> NodeId {
        let l = self.endless_definition_map(&def_t, &None, &name_t, &assignment_t, body);
        let mut children = vec![Child::Sym(value(&name_t).to_owned())];
        children.extend(Self::nodes_children(&[Some(args), body]));
        self.n(NodeType::Def, children, l)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn def_singleton(
        &mut self,
        def_t: T,
        definee: NodeId,
        dot_t: T,
        name_t: T,
        args: NodeId,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.definition_map(&def_t, &dot_t, &name_t, &end_t);
        let children = vec![
            Child::Node(definee),
            Child::Sym(value(&name_t).to_owned()),
            Child::Node(args),
            body.map_or(Child::Nil, Child::Node),
        ];
        self.n(NodeType::Defs, children, l)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn def_endless_singleton(
        &mut self,
        def_t: T,
        definee: NodeId,
        dot_t: T,
        name_t: T,
        args: NodeId,
        assignment_t: T,
        body: Option<NodeId>,
    ) -> NodeId {
        let l = self.endless_definition_map(&def_t, &dot_t, &name_t, &assignment_t, body);
        let children = vec![
            Child::Node(definee),
            Child::Sym(value(&name_t).to_owned()),
            Child::Node(args),
            body.map_or(Child::Nil, Child::Node),
        ];
        self.n(NodeType::Defs, children, l)
    }

    pub fn undef_method(&mut self, undef_t: T, names: Vec<NodeId>) -> NodeId {
        let opt: Vec<Option<NodeId>> = names.iter().copied().map(Some).collect();
        let l = self.keyword_map(&undef_t, &None, &opt, &None);
        self.n(NodeType::Undef, Self::some_children(&names), l)
    }

    pub fn alias(&mut self, alias_t: T, to: NodeId, from: NodeId) -> NodeId {
        let l = self.keyword_map(&alias_t, &None, &[Some(to), Some(from)], &None);
        self.n(NodeType::Alias, vec![Child::Node(to), Child::Node(from)], l)
    }

    // ---- formal arguments ----------------------------------------------------

    pub fn args(&mut self, begin_t: T, args: Vec<NodeId>, end_t: T) -> NodeId {
        let l = self.collection_map(&begin_t, &args, &end_t);
        self.n(NodeType::Args, Self::some_children(&args), l)
    }

    pub fn numargs(&mut self, max: i64) -> NodeId {
        self.n(NodeType::Numargs, vec![Child::Int(max)], Loc::default())
    }

    pub fn itarg(&mut self) -> NodeId {
        self.n(
            NodeType::Itarg,
            vec![Child::Sym("it".to_owned())],
            Loc::default(),
        )
    }

    pub fn forward_arg(&mut self, dots_t: T) -> NodeId {
        let l = self.token_map(&dots_t);
        self.n0(NodeType::ForwardArg, l)
    }

    pub fn arg(&mut self, name_t: T) -> NodeId {
        let l = self.variable_map(&name_t);
        let v = value(&name_t).to_owned();
        self.n(NodeType::Arg, vec![Child::Sym(v)], l)
    }

    pub fn optarg(&mut self, name_t: T, eql_t: T, value_n: NodeId) -> NodeId {
        let mut l = self.variable_map(&name_t);
        l.operator = loc(&eql_t);
        l.expression = Some(join(loc(&name_t).unwrap(), self.expr(value_n)));
        let v = value(&name_t).to_owned();
        self.n(
            NodeType::Optarg,
            vec![Child::Sym(v), Child::Node(value_n)],
            l,
        )
    }

    pub fn restarg(&mut self, star_t: T, name_t: T) -> NodeId {
        let l = self.arg_prefix_map(&star_t, &name_t);
        match &name_t {
            Some(name) => {
                let v = name.value.clone();
                self.n(NodeType::Restarg, vec![Child::Sym(v)], l)
            }
            None => self.n0(NodeType::Restarg, l),
        }
    }

    pub fn kwarg(&mut self, name_t: T) -> NodeId {
        let l = self.kwarg_map(&name_t, None);
        let v = value(&name_t).to_owned();
        self.n(NodeType::Kwarg, vec![Child::Sym(v)], l)
    }

    pub fn kwoptarg(&mut self, name_t: T, value_n: NodeId) -> NodeId {
        let l = self.kwarg_map(&name_t, Some(value_n));
        let v = value(&name_t).to_owned();
        self.n(
            NodeType::Kwoptarg,
            vec![Child::Sym(v), Child::Node(value_n)],
            l,
        )
    }

    pub fn kwrestarg(&mut self, dstar_t: T, name_t: T) -> NodeId {
        let l = self.arg_prefix_map(&dstar_t, &name_t);
        match &name_t {
            Some(name) => {
                let v = name.value.clone();
                self.n(NodeType::Kwrestarg, vec![Child::Sym(v)], l)
            }
            None => self.n0(NodeType::Kwrestarg, l),
        }
    }

    pub fn kwnilarg(&mut self, dstar_t: T, nil_t: T) -> NodeId {
        let l = self.arg_prefix_map(&dstar_t, &nil_t);
        self.n0(NodeType::Kwnilarg, l)
    }

    pub fn shadowarg(&mut self, name_t: T) -> NodeId {
        let l = self.variable_map(&name_t);
        let v = value(&name_t).to_owned();
        self.n(NodeType::Shadowarg, vec![Child::Sym(v)], l)
    }

    pub fn blockarg(&mut self, amper_t: T, name_t: T) -> NodeId {
        let l = self.arg_prefix_map(&amper_t, &name_t);
        let child = match &name_t {
            Some(name) => Child::Sym(name.value.clone()),
            None => Child::Nil,
        };
        self.n(NodeType::Blockarg, vec![child], l)
    }

    // ---- method calls ----------------------------------------------------------

    fn call_type_for_dot(dot_t: &T) -> NodeType {
        if dot_t.as_ref().is_some_and(|t| t.value == "&.") {
            NodeType::Csend
        } else {
            NodeType::Send
        }
    }

    pub fn forwarded_args(&mut self, dots_t: T) -> NodeId {
        let l = self.token_map(&dots_t);
        self.n0(NodeType::ForwardedArgs, l)
    }

    pub fn forwarded_restarg(&mut self, star_t: T) -> NodeId {
        let l = self.token_map(&star_t);
        self.n0(NodeType::ForwardedRestarg, l)
    }

    pub fn forwarded_kwrestarg(&mut self, dstar_t: T) -> NodeId {
        let l = self.token_map(&dstar_t);
        self.n0(NodeType::ForwardedKwrestarg, l)
    }

    pub fn call_method(
        &mut self,
        receiver: Option<NodeId>,
        dot_t: T,
        selector_t: T,
        lparen_t: T,
        args: Vec<NodeId>,
        rparen_t: T,
    ) -> NodeId {
        let ty = Self::call_type_for_dot(&dot_t);
        let l = self.send_map(receiver, &dot_t, &selector_t, &lparen_t, &args, &rparen_t);
        let name = match &selector_t {
            None => "call".to_owned(),
            Some(selector) => selector.value.clone(),
        };
        let mut children = vec![receiver.map_or(Child::Nil, Child::Node), Child::Sym(name)];
        children.extend(Self::some_children(&args));
        self.n(ty, children, l)
    }

    pub fn call_lambda(&mut self, lambda_t: T) -> NodeId {
        let l = self.send_map(None, &None, &lambda_t, &None, &[], &None);
        self.n(
            NodeType::Send,
            vec![Child::Nil, Child::Sym("lambda".to_owned())],
            l,
        )
    }

    /// `Prism::Translation::Parser::Builder#block`.
    pub fn block(
        &mut self,
        method_call: NodeId,
        begin_t: T,
        args: NodeId,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let (block_type, args_child) = match self.ty(args) {
            NodeType::Itarg => (NodeType::Itblock, Child::Sym("it".to_owned())),
            NodeType::Numargs => (NodeType::Numblock, self.node(args).children[0].clone()),
            _ => (NodeType::Block, Child::Node(args)),
        };
        let call_type = self.ty(method_call);
        if matches!(
            call_type,
            NodeType::Send
                | NodeType::Csend
                | NodeType::Index
                | NodeType::Super
                | NodeType::Zsuper
                | NodeType::Lambda
        ) {
            let l = self.block_map(self.expr(method_call), &begin_t, &end_t);
            self.n(
                block_type,
                vec![
                    Child::Node(method_call),
                    args_child,
                    body.map_or(Child::Nil, Child::Node),
                ],
                l,
            )
        } else {
            // Code like "return foo 1 do end" is reduced in a weird sequence.
            // Here, method_call is actually (return).
            let actual_send = self.first_child_node(method_call).unwrap();
            let l = self.block_map(self.expr(actual_send), &begin_t, &end_t);
            let block = self.n(
                block_type,
                vec![
                    Child::Node(actual_send),
                    args_child,
                    body.map_or(Child::Nil, Child::Node),
                ],
                l,
            );
            let expr = self.join_exprs(method_call, block);
            let node = self.node_mut(method_call);
            node.children = vec![Child::Node(block)];
            node.loc.expression = Some(expr);
            method_call
        }
    }

    pub fn block_pass(&mut self, amper_t: T, arg: Option<NodeId>) -> NodeId {
        let l = self.unary_op_map(&amper_t, arg);
        self.n(NodeType::BlockPass, Self::nodes_children(&[arg]), l)
    }

    pub fn attr_asgn(&mut self, receiver: Option<NodeId>, dot_t: T, selector_t: T) -> NodeId {
        let method_name = format!("{}=", value(&selector_t));
        let ty = Self::call_type_for_dot(&dot_t);
        let l = self.send_map(receiver, &dot_t, &selector_t, &None, &[], &None);
        self.n(
            ty,
            vec![
                receiver.map_or(Child::Nil, Child::Node),
                Child::Sym(method_name),
            ],
            l,
        )
    }

    pub fn index(
        &mut self,
        receiver: NodeId,
        lbrack_t: T,
        indexes: Vec<NodeId>,
        rbrack_t: T,
    ) -> NodeId {
        let l = self.send_index_map(receiver, &lbrack_t, &rbrack_t);
        let mut children = vec![Child::Node(receiver), Child::Sym("[]".to_owned())];
        children.extend(Self::some_children(&indexes));
        self.n(NodeType::Send, children, l)
    }

    pub fn index_asgn(
        &mut self,
        receiver: NodeId,
        lbrack_t: T,
        indexes: Vec<NodeId>,
        rbrack_t: T,
    ) -> NodeId {
        let l = self.send_index_map(receiver, &lbrack_t, &rbrack_t);
        let mut children = vec![Child::Node(receiver), Child::Sym("[]=".to_owned())];
        children.extend(Self::some_children(&indexes));
        self.n(NodeType::Send, children, l)
    }

    /// `/re/ =~ x` with a static regexp declares its named captures.
    pub fn match_op(&mut self, receiver: NodeId, match_t: T, arg: NodeId) -> NodeId {
        let l = self.send_binary_op_map(receiver, &match_t, arg);
        if self.static_regexp_node(receiver) {
            self.n(
                NodeType::MatchWithLvasgn,
                vec![Child::Node(receiver), Child::Node(arg)],
                l,
            )
        } else {
            self.n(
                NodeType::Send,
                vec![
                    Child::Node(receiver),
                    Child::Sym("=~".to_owned()),
                    Child::Node(arg),
                ],
                l,
            )
        }
    }

    fn static_regexp_node(&self, node: NodeId) -> bool {
        let node = self.node(node);
        node.ty == NodeType::Regexp
            && node.children[..node.children.len() - 1]
                .iter()
                .all(|child| child.as_node().is_some_and(|c| self.ty(c) == NodeType::Str))
    }

    pub fn not_op(&mut self, not_t: T, begin_t: T, receiver: Option<NodeId>, end_t: T) -> NodeId {
        match receiver {
            None => {
                let bl = self.collection_map(&begin_t, &[], &end_t);
                let nil_node = self.n0(NodeType::Begin, bl);
                let l = self.send_unary_op_map(&not_t, Some(nil_node));
                self.n(
                    NodeType::Send,
                    vec![Child::Node(nil_node), Child::Sym("!".to_owned())],
                    l,
                )
            }
            Some(receiver) => {
                let receiver = self.check_condition(receiver);
                let l = self.send_map(None, &None, &not_t, &begin_t, &[receiver], &end_t);
                self.n(
                    NodeType::Send,
                    vec![Child::Node(receiver), Child::Sym("!".to_owned())],
                    l,
                )
            }
        }
    }

    // ---- control flow ----------------------------------------------------------

    pub fn logical_op(&mut self, ty: NodeType, lhs: NodeId, op_t: T, rhs: NodeId) -> NodeId {
        let l = self.binary_op_map(lhs, &op_t, rhs);
        self.n(ty, vec![Child::Node(lhs), Child::Node(rhs)], l)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn condition(
        &mut self,
        cond_t: T,
        cond: NodeId,
        then_t: T,
        if_true: Option<NodeId>,
        else_t: T,
        if_false: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.condition_map(
            &cond_t,
            Some(cond),
            &then_t,
            if_true,
            &else_t,
            if_false,
            &end_t,
        );
        let cond = self.check_condition(cond);
        self.n(
            NodeType::If,
            Self::nodes_children(&[Some(cond), if_true, if_false]),
            l,
        )
    }

    pub fn condition_mod(
        &mut self,
        if_true: Option<NodeId>,
        if_false: Option<NodeId>,
        cond_t: T,
        cond: NodeId,
    ) -> NodeId {
        let l = self.keyword_mod_map(if_true.or(if_false).unwrap(), &cond_t, cond);
        let cond = self.check_condition(cond);
        self.n(
            NodeType::If,
            Self::nodes_children(&[Some(cond), if_true, if_false]),
            l,
        )
    }

    pub fn ternary(
        &mut self,
        cond: NodeId,
        question_t: T,
        if_true: NodeId,
        colon_t: T,
        if_false: NodeId,
    ) -> NodeId {
        let l = self.ternary_map(cond, &question_t, &colon_t, if_false);
        let cond = self.check_condition(cond);
        self.n(
            NodeType::If,
            vec![
                Child::Node(cond),
                Child::Node(if_true),
                Child::Node(if_false),
            ],
            l,
        )
    }

    pub fn when(
        &mut self,
        when_t: T,
        patterns: Vec<NodeId>,
        then_t: T,
        body: Option<NodeId>,
    ) -> NodeId {
        let mut children: Vec<Option<NodeId>> = patterns.into_iter().map(Some).collect();
        children.push(body);
        let l = self.keyword_map(&when_t, &then_t, &children, &None);
        self.n(NodeType::When, Self::nodes_children(&children), l)
    }

    pub fn case(
        &mut self,
        case_t: T,
        expr: Option<NodeId>,
        when_bodies: Vec<NodeId>,
        else_t: T,
        else_body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.condition_map(&case_t, expr, &None, None, &else_t, else_body, &end_t);
        let mut children = vec![expr];
        children.extend(when_bodies.into_iter().map(Some));
        children.push(else_body);
        self.n(NodeType::Case, Self::nodes_children(&children), l)
    }

    pub fn loop_(
        &mut self,
        ty: NodeType,
        keyword_t: T,
        cond: NodeId,
        do_t: T,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.keyword_map(&keyword_t, &do_t, &[], &end_t);
        let cond = self.check_condition(cond);
        self.n(ty, Self::nodes_children(&[Some(cond), body]), l)
    }

    pub fn loop_mod(&mut self, ty: NodeType, body: NodeId, keyword_t: T, cond: NodeId) -> NodeId {
        let ty = match (ty, self.ty(body)) {
            (NodeType::While, NodeType::Kwbegin) => NodeType::WhilePost,
            (NodeType::Until, NodeType::Kwbegin) => NodeType::UntilPost,
            (ty, _) => ty,
        };
        let l = self.keyword_mod_map(body, &keyword_t, cond);
        let cond = self.check_condition(cond);
        self.n(ty, vec![Child::Node(cond), Child::Node(body)], l)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn for_(
        &mut self,
        for_t: T,
        iterator: NodeId,
        in_t: T,
        iteratee: NodeId,
        do_t: T,
        body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let l = self.for_map(&for_t, &in_t, &do_t, &end_t);
        self.n(
            NodeType::For,
            Self::nodes_children(&[Some(iterator), Some(iteratee), body]),
            l,
        )
    }

    pub fn keyword_cmd(
        &mut self,
        ty: NodeType,
        keyword_t: T,
        lparen_t: T,
        args: Vec<NodeId>,
        rparen_t: T,
    ) -> NodeId {
        let opt: Vec<Option<NodeId>> = args.iter().copied().map(Some).collect();
        let l = self.keyword_map(&keyword_t, &lparen_t, &opt, &rparen_t);
        self.n(ty, Self::some_children(&args), l)
    }

    pub fn preexe(
        &mut self,
        ty: NodeType,
        preexe_t: T,
        lbrace_t: T,
        compstmt: Option<NodeId>,
        rbrace_t: T,
    ) -> NodeId {
        let l = self.keyword_map(&preexe_t, &lbrace_t, &[], &rbrace_t);
        self.n(ty, Self::nodes_children(&[compstmt]), l)
    }

    // ---- exception handling ----------------------------------------------------

    pub fn rescue_body(
        &mut self,
        rescue_t: T,
        exc_list: Option<NodeId>,
        assoc_t: T,
        exc_var: Option<NodeId>,
        then_t: T,
        compound_stmt: Option<NodeId>,
    ) -> NodeId {
        let l = self.rescue_body_map(
            &rescue_t,
            exc_list,
            &assoc_t,
            exc_var,
            &then_t,
            compound_stmt,
        );
        self.n(
            NodeType::Resbody,
            Self::nodes_children(&[exc_list, exc_var, compound_stmt]),
            l,
        )
    }

    pub fn begin_body(
        &mut self,
        compound_stmt: Option<NodeId>,
        rescue_bodies: Vec<NodeId>,
        else_t: T,
        else_: Option<NodeId>,
        ensure_t: T,
        ensure_: Option<NodeId>,
    ) -> Option<NodeId> {
        let mut compound_stmt = compound_stmt;
        if !rescue_bodies.is_empty() {
            let mut children: Vec<Option<NodeId>> = vec![compound_stmt];
            children.extend(rescue_bodies.iter().copied().map(Some));
            let bodies: Vec<Option<NodeId>> = rescue_bodies.iter().copied().map(Some).collect();
            let l = if else_t.is_some() {
                children.push(else_);
                self.eh_keyword_map(compound_stmt, &None, &bodies, &else_t, else_)
            } else {
                children.push(None);
                self.eh_keyword_map(compound_stmt, &None, &bodies, &None, None)
            };
            compound_stmt = Some(self.n(NodeType::Rescue, Self::nodes_children(&children), l));
        } else if else_t.is_some() {
            let mut statements: Vec<NodeId> = Vec::new();
            if let Some(stmt) = compound_stmt {
                if self.ty(stmt) == NodeType::Begin {
                    statements.extend(self.node(stmt).children.iter().filter_map(Child::as_node));
                } else {
                    statements.push(stmt);
                }
            }
            let else_parts: Vec<NodeId> = else_.into_iter().collect();
            let l = self.collection_map(&else_t, &else_parts, &None);
            let else_node = self.n(NodeType::Begin, Self::nodes_children(&[else_]), l);
            statements.push(else_node);
            let l = self.collection_map(&None, &statements, &None);
            compound_stmt = Some(self.n(NodeType::Begin, Self::some_children(&statements), l));
        }

        if ensure_t.is_some() {
            let l = self.eh_keyword_map(compound_stmt, &ensure_t, &[ensure_], &None, None);
            compound_stmt = Some(self.n(
                NodeType::Ensure,
                Self::nodes_children(&[compound_stmt, ensure_]),
                l,
            ));
        }

        compound_stmt
    }

    // ---- expression grouping ---------------------------------------------------

    pub fn compstmt(&mut self, statements: Vec<NodeId>) -> Option<NodeId> {
        match statements.len() {
            0 => None,
            1 => Some(statements[0]),
            _ => {
                let l = self.collection_map(&None, &statements, &None);
                Some(self.n(NodeType::Begin, Self::some_children(&statements), l))
            }
        }
    }

    fn is_bare_begin(&self, node: NodeId) -> bool {
        let n = self.node(node);
        n.ty == NodeType::Begin && n.loc.begin.is_none() && n.loc.end.is_none()
    }

    pub fn begin(&mut self, begin_t: T, body: Option<NodeId>, end_t: T) -> NodeId {
        match body {
            None => {
                let l = self.collection_map(&begin_t, &[], &end_t);
                self.n0(NodeType::Begin, l)
            }
            Some(body) if self.ty(body) == NodeType::Mlhs || self.is_bare_begin(body) => {
                let children = self.node(body).children.clone();
                let parts: Vec<NodeId> = children.iter().filter_map(Child::as_node).collect();
                let l = self.collection_map(&begin_t, &parts, &end_t);
                let ty = self.ty(body);
                self.n(ty, children, l)
            }
            Some(body) => {
                let l = self.collection_map(&begin_t, &[body], &end_t);
                self.n(NodeType::Begin, vec![Child::Node(body)], l)
            }
        }
    }

    pub fn begin_keyword(&mut self, begin_t: T, body: Option<NodeId>, end_t: T) -> NodeId {
        match body {
            None => {
                let l = self.collection_map(&begin_t, &[], &end_t);
                self.n0(NodeType::Kwbegin, l)
            }
            Some(body) if self.is_bare_begin(body) => {
                let children = self.node(body).children.clone();
                let parts: Vec<NodeId> = children.iter().filter_map(Child::as_node).collect();
                let l = self.collection_map(&begin_t, &parts, &end_t);
                self.n(NodeType::Kwbegin, children, l)
            }
            Some(body) => {
                let l = self.collection_map(&begin_t, &[body], &end_t);
                self.n(NodeType::Kwbegin, vec![Child::Node(body)], l)
            }
        }
    }

    // ---- pattern matching ------------------------------------------------------

    pub fn case_match(
        &mut self,
        case_t: T,
        expr: Option<NodeId>,
        in_bodies: Vec<NodeId>,
        else_t: T,
        else_body: Option<NodeId>,
        end_t: T,
    ) -> NodeId {
        let else_body = if else_t.is_some() && else_body.is_none() {
            let l = self.token_map(&else_t);
            Some(self.n0(NodeType::EmptyElse, l))
        } else {
            else_body
        };
        let l = self.condition_map(&case_t, expr, &None, None, &else_t, else_body, &end_t);
        let mut children = vec![expr];
        children.extend(in_bodies.into_iter().map(Some));
        children.push(else_body);
        self.n(NodeType::CaseMatch, Self::nodes_children(&children), l)
    }

    pub fn match_pattern(&mut self, ty: NodeType, lhs: NodeId, match_t: T, rhs: NodeId) -> NodeId {
        let l = self.binary_op_map(lhs, &match_t, rhs);
        self.n(ty, vec![Child::Node(lhs), Child::Node(rhs)], l)
    }

    pub fn in_pattern(
        &mut self,
        in_t: T,
        pattern: NodeId,
        guard: Option<NodeId>,
        then_t: T,
        body: Option<NodeId>,
    ) -> NodeId {
        let children = [Some(pattern), guard, body];
        let compact: Vec<Option<NodeId>> = children.iter().copied().flatten().map(Some).collect();
        let l = self.keyword_map(&in_t, &then_t, &compact, &None);
        self.n(NodeType::InPattern, Self::nodes_children(&children), l)
    }

    pub fn guard(&mut self, ty: NodeType, keyword_t: T, body: NodeId) -> NodeId {
        let l = self.guard_map(&keyword_t, body);
        self.n(ty, vec![Child::Node(body)], l)
    }

    pub fn match_var(&mut self, name_t: T) -> NodeId {
        let l = self.variable_map(&name_t);
        let v = value(&name_t).to_owned();
        self.n(NodeType::MatchVar, vec![Child::Sym(v)], l)
    }

    pub fn match_hash_var(&mut self, name_t: T) -> NodeId {
        let expr_l = loc(&name_t).unwrap();
        let name_l = Range::new(expr_l.begin_pos, expr_l.end_pos - 1);
        let v = value(&name_t).to_owned();
        self.n(
            NodeType::MatchVar,
            vec![Child::Sym(v)],
            Loc {
                name: Some(name_l),
                ..Loc::new(MapKind::Variable, Some(expr_l))
            },
        )
    }

    pub fn match_hash_var_from_str(
        &mut self,
        begin_t: T,
        strings: Vec<NodeId>,
        end_t: T,
    ) -> NodeId {
        let string = strings[0];
        match self.ty(string) {
            NodeType::Str => {
                let node = self.node(string);
                let name = match node.children.first().and_then(Child::str_bytes) {
                    Some(s) => String::from_utf8_lossy(s).into_owned(),
                    None => String::new(),
                };
                let mut name_l = node.loc.expression.unwrap();
                if let Some(begin_l) = node.loc.begin {
                    name_l.begin_pos += begin_l.len();
                }
                if let Some(end_l) = node.loc.end {
                    name_l.end_pos -= end_l.len();
                }
                let expr_l = join(
                    join(loc(&begin_t).unwrap(), node.loc.expression.unwrap()),
                    loc(&end_t).unwrap(),
                );
                self.n(
                    NodeType::MatchVar,
                    vec![Child::Sym(name)],
                    Loc {
                        name: Some(name_l),
                        ..Loc::new(MapKind::Variable, Some(expr_l))
                    },
                )
            }
            _ => {
                let children: Vec<NodeId> = self
                    .node(string)
                    .children
                    .iter()
                    .filter_map(Child::as_node)
                    .collect();
                self.match_hash_var_from_str(begin_t, children, end_t)
            }
        }
    }

    pub fn match_rest(&mut self, star_t: T, name_t: T) -> NodeId {
        match name_t {
            None => {
                let l = self.unary_op_map(&star_t, None);
                self.n0(NodeType::MatchRest, l)
            }
            Some(_) => {
                let name = self.match_var(name_t);
                let l = self.unary_op_map(&star_t, Some(name));
                self.n(NodeType::MatchRest, vec![Child::Node(name)], l)
            }
        }
    }

    pub fn hash_pattern(&mut self, lbrace_t: T, kwargs: Vec<NodeId>, rbrace_t: T) -> NodeId {
        let l = self.collection_map(&lbrace_t, &kwargs, &rbrace_t);
        self.n(NodeType::HashPattern, Self::some_children(&kwargs), l)
    }

    pub fn array_pattern(&mut self, lbrack_t: T, elements: Vec<NodeId>, rbrack_t: T) -> NodeId {
        let mut trailing_comma = false;
        let node_elements: Vec<NodeId> = elements
            .iter()
            .map(|&element| {
                if self.ty(element) == NodeType::MatchWithTrailingComma {
                    trailing_comma = true;
                    self.first_child_node(element).unwrap()
                } else {
                    trailing_comma = false;
                    element
                }
            })
            .collect();
        let ty = if trailing_comma {
            NodeType::ArrayPatternWithTail
        } else {
            NodeType::ArrayPattern
        };
        let l = self.collection_map(&lbrack_t, &elements, &rbrack_t);
        self.n(ty, Self::some_children(&node_elements), l)
    }

    pub fn find_pattern(&mut self, lbrack_t: T, elements: Vec<NodeId>, rbrack_t: T) -> NodeId {
        let l = self.collection_map(&lbrack_t, &elements, &rbrack_t);
        self.n(NodeType::FindPattern, Self::some_children(&elements), l)
    }

    pub fn match_with_trailing_comma(&mut self, matched: NodeId, comma_t: T) -> NodeId {
        let l = self.expr_map(join(self.expr(matched), loc(&comma_t).unwrap()));
        self.n(
            NodeType::MatchWithTrailingComma,
            vec![Child::Node(matched)],
            l,
        )
    }

    pub fn const_pattern(
        &mut self,
        const_n: NodeId,
        ldelim_t: T,
        pattern: NodeId,
        rdelim_t: T,
    ) -> NodeId {
        let l = Loc {
            begin: loc(&ldelim_t),
            end: loc(&rdelim_t),
            ..Loc::new(
                MapKind::Collection,
                Some(join(self.expr(const_n), loc(&rdelim_t).unwrap())),
            )
        };
        self.n(
            NodeType::ConstPattern,
            vec![Child::Node(const_n), Child::Node(pattern)],
            l,
        )
    }

    pub fn pin(&mut self, pin_t: T, var: NodeId) -> NodeId {
        let l = self.send_unary_op_map(&pin_t, Some(var));
        self.n(NodeType::Pin, vec![Child::Node(var)], l)
    }

    pub fn match_binary(&mut self, ty: NodeType, left: NodeId, op_t: T, right: NodeId) -> NodeId {
        let l = self.binary_op_map(left, &op_t, right);
        self.n(ty, vec![Child::Node(left), Child::Node(right)], l)
    }

    pub fn match_nil_pattern(&mut self, dstar_t: T, nil_t: T) -> NodeId {
        let l = self.arg_prefix_map(&dstar_t, &nil_t);
        self.n0(NodeType::MatchNilPattern, l)
    }

    // ---- helpers -----------------------------------------------------------------

    fn check_condition(&mut self, cond: NodeId) -> NodeId {
        match self.ty(cond) {
            NodeType::Begin => {
                if self.node(cond).children.len() == 1
                    && let Some(child) = self.node(cond).children[0].as_node()
                {
                    let checked = self.check_condition(child);
                    self.node_mut(cond).children[0] = Child::Node(checked);
                }
                cond
            }
            NodeType::And | NodeType::Or => {
                let lhs = self.node(cond).children[0].as_node().unwrap();
                let rhs = self.node(cond).children[1].as_node().unwrap();
                let lhs = self.check_condition(lhs);
                let rhs = self.check_condition(rhs);
                self.node_mut(cond).children = vec![Child::Node(lhs), Child::Node(rhs)];
                cond
            }
            ty @ (NodeType::Irange | NodeType::Erange) => {
                let lhs = self.node(cond).children[0].as_node();
                let rhs = self.node(cond).children[1].as_node();
                let lhs = lhs.map(|l| self.check_condition(l));
                let rhs = rhs.map(|r| self.check_condition(r));
                let node = self.node_mut(cond);
                node.ty = if ty == NodeType::Irange {
                    NodeType::Iflipflop
                } else {
                    NodeType::Eflipflop
                };
                node.children = Self::nodes_children(&[lhs, rhs]);
                cond
            }
            NodeType::Regexp => {
                let l = self.expr_map(self.expr(cond));
                self.n(NodeType::MatchCurrentLine, vec![Child::Node(cond)], l)
            }
            _ => cond,
        }
    }
}
