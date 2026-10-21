//! Port of `Prism::Translation::Parser::Compiler` (prism 1.9): converts a
//! Prism tree into the `parser` gem's tree, through the builder port.

use ruby_prism::{Location, Node};

use crate::ast::builder::{BuildNode, Builder, T, Tok, join};
use crate::ast::{Child, NodeId, NodeType};
use crate::processed_source::Range;

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn range(location: &Location<'_>) -> Range {
    Range::new(location.start() as usize, location.end() as usize)
}

fn token(location: Option<Location<'_>>) -> T {
    location.map(|l| Tok::new(lossy(l.as_slice()), range(&l)))
}

fn tok(value: impl Into<String>, location: &Location<'_>) -> T {
    Some(Tok::new(value, range(location)))
}

/// Ruby's `String#lines` on bytes: splits after each newline.
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            out.push(&bytes[start..=i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

/// The number of backslashes right before a trailing `\r?\n`.
fn trailing_backslashes(line: &[u8]) -> usize {
    let mut end = line.len();
    if end == 0 || line[end - 1] != b'\n' {
        return 0;
    }
    end -= 1;
    if end > 0 && line[end - 1] == b'\r' {
        end -= 1;
    }
    line[..end]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
}

/// The length of `string[/\s+$/]`.
fn trailing_space_match(s: &[u8]) -> usize {
    for i in 0..s.len() {
        if !is_space(s[i]) {
            continue;
        }
        let mut j = i;
        while j < s.len() && is_space(s[j]) {
            j += 1;
        }
        let mut k = j;
        while k > i {
            if k == s.len() || s[k] == b'\n' {
                return k - i;
            }
            k -= 1;
        }
    }
    0
}

fn chomp(s: &[u8]) -> &[u8] {
    if let Some(rest) = s.strip_suffix(b"\r\n") {
        rest
    } else if let Some(rest) = s.strip_suffix(b"\n") {
        rest
    } else if let Some(rest) = s.strip_suffix(b"\r") {
        rest
    } else {
        s
    }
}

fn integer_value(integer: &ruby_prism::Integer<'_>) -> Child {
    let (negative, digits) = integer.to_u32_digits();
    let mut value: u128 = 0;
    let mut fits = digits.len() <= 4;
    if fits {
        for &digit in digits.iter().rev() {
            value = (value << 32) | u128::from(digit);
        }
        fits = value <= i64::MAX as u128 + u128::from(negative);
    }
    if fits {
        let v = if negative {
            (value as i128).wrapping_neg() as i64
        } else {
            value as i64
        };
        return Child::Int(v);
    }
    // Arbitrary precision: repeated division of the base-2^32 digits.
    let mut digits: Vec<u32> = digits.to_vec();
    let mut decimal = Vec::new();
    while digits.iter().any(|&d| d != 0) {
        let mut rem: u64 = 0;
        for d in digits.iter_mut().rev() {
            let cur = (rem << 32) | u64::from(*d);
            *d = (cur / 10) as u32;
            rem = cur % 10;
        }
        decimal.push(b'0' + rem as u8);
    }
    if decimal.is_empty() {
        decimal.push(b'0');
    }
    if negative {
        decimal.push(b'-');
    }
    decimal.reverse();
    Child::BigInt(String::from_utf8(decimal).unwrap())
}

fn integer_string(integer: &ruby_prism::Integer<'_>) -> String {
    match integer_value(integer) {
        Child::Int(i) => i.to_string(),
        Child::BigInt(s) => s,
        _ => unreachable!(),
    }
}

fn negate(value: Child) -> Child {
    match value {
        Child::Int(i) => i
            .checked_neg()
            .map_or_else(|| Child::BigInt(format!("{}", -(i as i128))), Child::Int),
        Child::BigInt(s) => Child::BigInt(match s.strip_prefix('-') {
            Some(rest) => rest.to_owned(),
            None => format!("-{s}"),
        }),
        Child::Float(f) => Child::Float(-f),
        Child::Rational(n, d) => Child::Rational(
            match n.strip_prefix('-') {
                Some(rest) => rest.to_owned(),
                None if n == "0" => n,
                None => format!("-{n}"),
            },
            d,
        ),
        Child::Complex(imaginary) => Child::Complex(Box::new(negate(*imaginary))),
        other => other,
    }
}

/// A heredoc part: a Prism node, or the single string part of a
/// non-interpolated heredoc (`StringNode#to_interpolated`).
enum Part<'pr> {
    Node(Node<'pr>),
    Str {
        unescaped: &'pr [u8],
        content: &'pr [u8],
        location: Location<'pr>,
    },
}

#[derive(Clone)]
struct State {
    forwarding: Vec<&'static str>,
    in_destructure: bool,
    in_pattern: bool,
}

pub struct Compiler<'s> {
    builder: Builder<'s>,
    state: State,
}

impl<'s> Compiler<'s> {
    pub fn new(source: &'s str, file_name: &str) -> Self {
        Self {
            builder: Builder::new(source, file_name),
            state: State {
                forwarding: Vec::new(),
                in_destructure: false,
                in_pattern: false,
            },
        }
    }

    pub fn compile(&mut self, program: &Node<'_>) -> Option<NodeId> {
        self.visit(program)
    }

    pub fn into_tree(self) -> Vec<BuildNode> {
        self.builder.nodes
    }

    fn source(&self) -> &'s [u8] {
        self.builder.source.as_bytes()
    }

    fn with_state<R>(&mut self, state: State, f: impl FnOnce(&mut Self) -> R) -> R {
        let saved = std::mem::replace(&mut self.state, state);
        let result = f(self);
        self.state = saved;
        result
    }

    fn with_forwarding<R>(
        &mut self,
        forwarding: Vec<&'static str>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let state = State {
            forwarding,
            ..self.state.clone()
        };
        self.with_state(state, f)
    }

    fn with_destructure<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let state = State {
            in_destructure: true,
            ..self.state.clone()
        };
        self.with_state(state, f)
    }

    fn with_pattern<R>(&mut self, in_pattern: bool, f: impl FnOnce(&mut Self) -> R) -> R {
        let state = State {
            in_pattern,
            ..self.state.clone()
        };
        self.with_state(state, f)
    }

    fn visit_opt(&mut self, node: Option<Node<'_>>) -> Option<NodeId> {
        node.and_then(|n| self.visit(&n))
    }

    fn visit_all<'pr>(&mut self, nodes: impl IntoIterator<Item = Node<'pr>>) -> Vec<NodeId> {
        nodes.into_iter().filter_map(|n| self.visit(&n)).collect()
    }

    fn visit_arguments(&mut self, node: Option<ruby_prism::ArgumentsNode<'_>>) -> Vec<NodeId> {
        match node {
            Some(args) => self.visit_all(args.arguments().iter()),
            None => Vec::new(),
        }
    }

    fn srange_semicolon(&self, start: usize, end: Option<usize>) -> T {
        let source = self.source();
        let end = end.unwrap_or(source.len()).min(source.len());
        if start > end {
            return None;
        }
        let slice = &source[start..end];
        let ws = slice.iter().take_while(|&&b| is_space(b)).count();
        if slice.get(ws) == Some(&b';') {
            let final_offset = start + ws + 1;
            Some(Tok::new(";", Range::new(final_offset - 1, final_offset)))
        } else {
            None
        }
    }

    fn call_operator(location: Option<Location<'_>>) -> T {
        location.map(|l| {
            let value = match l.as_slice() {
                b"&." => "&.",
                b"::" => "::",
                _ => ".",
            };
            Tok::new(value, range(&l))
        })
    }

    // ---- the visitor -----------------------------------------------------------

    #[allow(clippy::too_many_lines)]
    fn visit(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let b = &mut self.builder;
        Some(match node {
            Node::AliasMethodNode { .. } => {
                let n = node.as_alias_method_node().unwrap();
                let to = self.visit(&n.new_name())?;
                let from = self.visit(&n.old_name())?;
                self.builder.alias(token(Some(n.keyword_loc())), to, from)
            }
            Node::AliasGlobalVariableNode { .. } => {
                let n = node.as_alias_global_variable_node().unwrap();
                let to = self.visit(&n.new_name())?;
                let from = self.visit(&n.old_name())?;
                self.builder.alias(token(Some(n.keyword_loc())), to, from)
            }
            Node::AlternationPatternNode { .. } => {
                let n = node.as_alternation_pattern_node().unwrap();
                let left = self.visit(&n.left())?;
                let right = self.visit(&n.right())?;
                self.builder.match_binary(
                    NodeType::MatchAlt,
                    left,
                    token(Some(n.operator_loc())),
                    right,
                )
            }
            Node::AndNode { .. } => {
                let n = node.as_and_node().unwrap();
                let left = self.visit(&n.left())?;
                let right = self.visit(&n.right())?;
                self.builder
                    .logical_op(NodeType::And, left, token(Some(n.operator_loc())), right)
            }
            Node::OrNode { .. } => {
                let n = node.as_or_node().unwrap();
                let left = self.visit(&n.left())?;
                let right = self.visit(&n.right())?;
                self.builder
                    .logical_op(NodeType::Or, left, token(Some(n.operator_loc())), right)
            }
            Node::ArrayNode { .. } => return Some(self.visit_array_node(node)),
            Node::ArrayPatternNode { .. } => return Some(self.visit_array_pattern_node(node)),
            Node::AssocNode { .. } => return self.visit_assoc_node(node),
            Node::AssocSplatNode { .. } => {
                let n = node.as_assoc_splat_node().unwrap();
                if self.state.in_pattern {
                    let name = n.value().map(|v| v.location());
                    self.builder
                        .match_rest(token(Some(n.operator_loc())), token(name))
                } else if n.value().is_none() && self.state.forwarding.contains(&"**") {
                    self.builder
                        .forwarded_kwrestarg(token(Some(n.operator_loc())))
                } else {
                    let value = self.visit_opt(n.value());
                    self.builder.kwsplat(token(Some(n.operator_loc())), value)
                }
            }
            Node::BackReferenceReadNode { .. } => b.back_ref(token(Some(node.location()))),
            Node::BeginNode { .. } => return self.visit_begin_node(node),
            Node::BlockArgumentNode { .. } => {
                let n = node.as_block_argument_node().unwrap();
                let expression = self.visit_opt(n.expression());
                self.builder
                    .block_pass(token(Some(n.operator_loc())), expression)
            }
            Node::BlockLocalVariableNode { .. } => b.shadowarg(token(Some(node.location()))),
            Node::NoBlockParameterNode { .. } => {
                let n = node.as_no_block_parameter_node().unwrap();
                b.blocknilarg(token(Some(n.operator_loc())), token(Some(n.keyword_loc())))
            }
            Node::BlockParameterNode { .. } => {
                let n = node.as_block_parameter_node().unwrap();
                b.blockarg(token(Some(n.operator_loc())), token(n.name_loc()))
            }
            Node::BreakNode { .. } => {
                let n = node.as_break_node().unwrap();
                let args = self.visit_arguments(n.arguments());
                self.builder.keyword_cmd(
                    NodeType::Break,
                    token(Some(n.keyword_loc())),
                    None,
                    args,
                    None,
                )
            }
            Node::NextNode { .. } => {
                let n = node.as_next_node().unwrap();
                let args = self.visit_arguments(n.arguments());
                self.builder.keyword_cmd(
                    NodeType::Next,
                    token(Some(n.keyword_loc())),
                    None,
                    args,
                    None,
                )
            }
            Node::ReturnNode { .. } => {
                let n = node.as_return_node().unwrap();
                let args = self.visit_arguments(n.arguments());
                self.builder.keyword_cmd(
                    NodeType::Return,
                    token(Some(n.keyword_loc())),
                    None,
                    args,
                    None,
                )
            }
            Node::CallNode { .. } => return Some(self.visit_call_node(node)),
            Node::CallOperatorWriteNode { .. } => {
                let n = node.as_call_operator_write_node().unwrap();
                let receiver = self.visit_opt(n.receiver());
                let lhs = self.builder.call_method(
                    receiver,
                    Self::call_operator(n.call_operator_loc()),
                    n.message_loc()
                        .map(|m| Tok::new(lossy(n.read_name().as_slice()), range(&m))),
                    None,
                    Vec::new(),
                    None,
                );
                let op = n.binary_operator_loc();
                let value = self.visit(&n.value())?;
                self.builder
                    .op_assign(lhs, tok(lossy(chomp_eq(op.as_slice())), &op), value)
            }
            Node::CallAndWriteNode { .. } => {
                let n = node.as_call_and_write_node().unwrap();
                let receiver = self.visit_opt(n.receiver());
                let lhs = self.builder.call_method(
                    receiver,
                    Self::call_operator(n.call_operator_loc()),
                    n.message_loc()
                        .map(|m| Tok::new(lossy(n.read_name().as_slice()), range(&m))),
                    None,
                    Vec::new(),
                    None,
                );
                let op = n.operator_loc();
                let value = self.visit(&n.value())?;
                self.builder
                    .op_assign(lhs, tok(lossy(chomp_eq(op.as_slice())), &op), value)
            }
            Node::CallOrWriteNode { .. } => {
                let n = node.as_call_or_write_node().unwrap();
                let receiver = self.visit_opt(n.receiver());
                let lhs = self.builder.call_method(
                    receiver,
                    Self::call_operator(n.call_operator_loc()),
                    n.message_loc()
                        .map(|m| Tok::new(lossy(n.read_name().as_slice()), range(&m))),
                    None,
                    Vec::new(),
                    None,
                );
                let op = n.operator_loc();
                let value = self.visit(&n.value())?;
                self.builder
                    .op_assign(lhs, tok(lossy(chomp_eq(op.as_slice())), &op), value)
            }
            Node::CallTargetNode { .. } => {
                let n = node.as_call_target_node().unwrap();
                let receiver = self.visit(&n.receiver());
                self.builder.attr_asgn(
                    receiver,
                    Self::call_operator(Some(n.call_operator_loc())),
                    token(Some(n.message_loc())),
                )
            }
            Node::CapturePatternNode { .. } => {
                let n = node.as_capture_pattern_node().unwrap();
                let value = self.visit(&n.value())?;
                let target = self.visit(&n.target().as_node())?;
                self.builder.match_binary(
                    NodeType::MatchAs,
                    value,
                    token(Some(n.operator_loc())),
                    target,
                )
            }
            Node::CaseNode { .. } => {
                let n = node.as_case_node().unwrap();
                let predicate = self.visit_opt(n.predicate());
                let conditions = self.visit_all(n.conditions().iter());
                let else_clause = n.else_clause();
                let else_t = token(else_clause.as_ref().map(|e| e.else_keyword_loc()));
                let else_body = else_clause
                    .as_ref()
                    .and_then(|e| e.statements())
                    .and_then(|s| self.visit(&s.as_node()));
                self.builder.case(
                    token(Some(n.case_keyword_loc())),
                    predicate,
                    conditions,
                    else_t,
                    else_body,
                    token(Some(n.end_keyword_loc())),
                )
            }
            Node::CaseMatchNode { .. } => {
                let n = node.as_case_match_node().unwrap();
                let predicate = self.visit_opt(n.predicate());
                let conditions = self.visit_all(n.conditions().iter());
                let else_clause = n.else_clause();
                let else_t = token(else_clause.as_ref().map(|e| e.else_keyword_loc()));
                let else_body = else_clause
                    .as_ref()
                    .and_then(|e| e.statements())
                    .and_then(|s| self.visit(&s.as_node()));
                self.builder.case_match(
                    token(Some(n.case_keyword_loc())),
                    predicate,
                    conditions,
                    else_t,
                    else_body,
                    token(Some(n.end_keyword_loc())),
                )
            }
            Node::ClassNode { .. } => {
                let n = node.as_class_node().unwrap();
                let name = self.visit(&n.constant_path())?;
                let superclass = self.visit_opt(n.superclass());
                let body = self.with_forwarding(Vec::new(), |c| c.visit_opt(n.body()));
                self.builder.def_class(
                    token(Some(n.class_keyword_loc())),
                    name,
                    token(n.inheritance_operator_loc()),
                    superclass,
                    body,
                    token(Some(n.end_keyword_loc())),
                )
            }
            Node::ModuleNode { .. } => {
                let n = node.as_module_node().unwrap();
                let name = self.visit(&n.constant_path())?;
                let body = self.with_forwarding(Vec::new(), |c| c.visit_opt(n.body()));
                self.builder.def_module(
                    token(Some(n.module_keyword_loc())),
                    name,
                    body,
                    token(Some(n.end_keyword_loc())),
                )
            }
            Node::SingletonClassNode { .. } => {
                let n = node.as_singleton_class_node().unwrap();
                let expr = self.visit(&n.expression())?;
                let body = self.with_forwarding(Vec::new(), |c| c.visit_opt(n.body()));
                self.builder.def_sclass(
                    token(Some(n.class_keyword_loc())),
                    token(Some(n.operator_loc())),
                    expr,
                    body,
                    token(Some(n.end_keyword_loc())),
                )
            }
            Node::ClassVariableReadNode { .. } => {
                b.variable(NodeType::Cvar, token(Some(node.location())))
            }
            Node::ClassVariableWriteNode { .. } => {
                let n = node.as_class_variable_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Cvar, token(Some(n.name_loc())));
                let lhs = self.builder.assignable(lhs);
                let value = self.visit(&n.value())?;
                self.builder
                    .assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::ClassVariableOperatorWriteNode { .. } => {
                let n = node.as_class_variable_operator_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Cvar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::ClassVariableAndWriteNode { .. } => {
                let n = node.as_class_variable_and_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Cvar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::ClassVariableOrWriteNode { .. } => {
                let n = node.as_class_variable_or_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Cvar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::ClassVariableTargetNode { .. } => {
                let lhs = b.variable(NodeType::Cvar, token(Some(node.location())));
                b.assignable(lhs)
            }
            Node::ConstantReadNode { .. } => b.const_(token(Some(node.location()))),
            Node::ConstantWriteNode { .. } => {
                let n = node.as_constant_write_node().unwrap();
                let lhs = self.builder.const_(token(Some(n.name_loc())));
                let lhs = self.builder.assignable(lhs);
                let value = self.visit(&n.value())?;
                self.builder
                    .assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::ConstantOperatorWriteNode { .. } => {
                let n = node.as_constant_operator_write_node().unwrap();
                let lhs = self.builder.const_(token(Some(n.name_loc())));
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::ConstantAndWriteNode { .. } => {
                let n = node.as_constant_and_write_node().unwrap();
                let lhs = self.builder.const_(token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::ConstantOrWriteNode { .. } => {
                let n = node.as_constant_or_write_node().unwrap();
                let lhs = self.builder.const_(token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::ConstantTargetNode { .. } => {
                let lhs = b.const_(token(Some(node.location())));
                b.assignable(lhs)
            }
            Node::ConstantPathNode { .. } => {
                let n = node.as_constant_path_node().unwrap();
                return self.constant_path(n.parent(), n.delimiter_loc(), n.name_loc());
            }
            Node::ConstantPathTargetNode { .. } => {
                let n = node.as_constant_path_target_node().unwrap();
                let lhs = self.constant_path(n.parent(), n.delimiter_loc(), n.name_loc())?;
                self.builder.assignable(lhs)
            }
            Node::ConstantPathWriteNode { .. } => {
                let n = node.as_constant_path_write_node().unwrap();
                let lhs = self.visit(&n.target().as_node())?;
                let lhs = self.builder.assignable(lhs);
                let value = self.visit(&n.value())?;
                self.builder
                    .assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::ConstantPathOperatorWriteNode { .. } => {
                let n = node.as_constant_path_operator_write_node().unwrap();
                let lhs = self.visit(&n.target().as_node())?;
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::ConstantPathAndWriteNode { .. } => {
                let n = node.as_constant_path_and_write_node().unwrap();
                let lhs = self.visit(&n.target().as_node())?;
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::ConstantPathOrWriteNode { .. } => {
                let n = node.as_constant_path_or_write_node().unwrap();
                let lhs = self.visit(&n.target().as_node())?;
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::DefNode { .. } => return self.visit_def_node(node),
            Node::DefinedNode { .. } => {
                let n = node.as_defined_node().unwrap();
                let value = self.visit(&n.value())?;
                let multiline = n.lparen_loc().is_some_and(|lparen| {
                    let start = n.keyword_loc().start() as usize;
                    self.source()[start..lparen.end() as usize].contains(&b'\n')
                });
                if multiline {
                    let begin = self.builder.begin(
                        token(n.lparen_loc()),
                        Some(value),
                        token(n.rparen_loc()),
                    );
                    self.builder.keyword_cmd(
                        NodeType::Defined,
                        token(Some(n.keyword_loc())),
                        None,
                        vec![begin],
                        None,
                    )
                } else {
                    self.builder.keyword_cmd(
                        NodeType::Defined,
                        token(Some(n.keyword_loc())),
                        token(n.lparen_loc()),
                        vec![value],
                        token(n.rparen_loc()),
                    )
                }
            }
            Node::ElseNode { .. } => {
                let n = node.as_else_node().unwrap();
                return n.statements().and_then(|s| self.visit(&s.as_node()));
            }
            Node::EmbeddedStatementsNode { .. } => {
                let n = node.as_embedded_statements_node().unwrap();
                let body = n.statements().and_then(|s| self.visit(&s.as_node()));
                self.builder.begin(
                    token(Some(n.opening_loc())),
                    body,
                    token(Some(n.closing_loc())),
                )
            }
            Node::EmbeddedVariableNode { .. } => {
                let n = node.as_embedded_variable_node().unwrap();
                return self.visit(&n.variable());
            }
            Node::FalseNode { .. } => b.false_(token(Some(node.location()))),
            Node::TrueNode { .. } => b.true_(token(Some(node.location()))),
            Node::NilNode { .. } => b.nil(token(Some(node.location()))),
            Node::SelfNode { .. } => b.self_(token(Some(node.location()))),
            Node::FindPatternNode { .. } => {
                let n = node.as_find_pattern_node().unwrap();
                let mut elements = vec![n.left().as_node()];
                elements.extend(n.requireds().iter());
                elements.push(n.right().as_node());
                let elements = self.visit_all(elements);
                match n.constant() {
                    Some(constant) => {
                        let constant = self.visit(&constant)?;
                        let pattern = self.builder.find_pattern(None, elements, None);
                        self.builder.const_pattern(
                            constant,
                            token(n.opening_loc()),
                            pattern,
                            token(n.closing_loc()),
                        )
                    }
                    None => self.builder.find_pattern(
                        token(n.opening_loc()),
                        elements,
                        token(n.closing_loc()),
                    ),
                }
            }
            Node::FloatNode { .. } => {
                let n = node.as_float_node().unwrap();
                return Some(self.visit_numeric(
                    NodeType::Float,
                    Child::Float(n.value()),
                    range(&node.location()),
                ));
            }
            Node::IntegerNode { .. } => {
                let n = node.as_integer_node().unwrap();
                return Some(self.visit_numeric(
                    NodeType::Int,
                    integer_value(&n.value()),
                    range(&node.location()),
                ));
            }
            Node::RationalNode { .. } => {
                let n = node.as_rational_node().unwrap();
                return Some(self.visit_numeric(
                    NodeType::Rational,
                    Child::Rational(
                        integer_string(&n.numerator()),
                        integer_string(&n.denominator()),
                    ),
                    range(&node.location()),
                ));
            }
            Node::ImaginaryNode { .. } => {
                let n = node.as_imaginary_node().unwrap();
                let value = Self::numeric_value(&n.numeric());
                return Some(self.visit_numeric(
                    NodeType::Complex,
                    Child::Complex(Box::new(value)),
                    range(&node.location()),
                ));
            }
            Node::ForNode { .. } => {
                let n = node.as_for_node().unwrap();
                let index = self.visit(&n.index())?;
                let collection = self.visit(&n.collection())?;
                let do_t = match n.do_keyword_loc() {
                    Some(do_loc) => token(Some(do_loc)),
                    None => {
                        let end = n
                            .statements()
                            .map(|s| s.location().start() as usize)
                            .unwrap_or_else(|| n.end_keyword_loc().start() as usize);
                        self.srange_semicolon(n.collection().location().end() as usize, Some(end))
                    }
                };
                let body = n.statements().and_then(|s| self.visit(&s.as_node()));
                self.builder.for_(
                    token(Some(n.for_keyword_loc())),
                    index,
                    token(Some(n.in_keyword_loc())),
                    collection,
                    do_t,
                    body,
                    token(Some(n.end_keyword_loc())),
                )
            }
            Node::ForwardingArgumentsNode { .. } => b.forwarded_args(token(Some(node.location()))),
            Node::ForwardingParameterNode { .. } => b.forward_arg(token(Some(node.location()))),
            Node::ForwardingSuperNode { .. } => {
                let n = node.as_forwarding_super_node().unwrap();
                let start = node.location().start() as usize;
                let call = self.builder.keyword_cmd(
                    NodeType::Zsuper,
                    Some(Tok::new("super", Range::new(start, start + 5))),
                    None,
                    Vec::new(),
                    None,
                );
                return Some(self.visit_block(call, n.block()));
            }
            Node::GlobalVariableReadNode { .. } => {
                b.variable(NodeType::Gvar, token(Some(node.location())))
            }
            Node::GlobalVariableWriteNode { .. } => {
                let n = node.as_global_variable_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Gvar, token(Some(n.name_loc())));
                let lhs = self.builder.assignable(lhs);
                let value = self.visit(&n.value())?;
                self.builder
                    .assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::GlobalVariableOperatorWriteNode { .. } => {
                let n = node.as_global_variable_operator_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Gvar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::GlobalVariableAndWriteNode { .. } => {
                let n = node.as_global_variable_and_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Gvar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::GlobalVariableOrWriteNode { .. } => {
                let n = node.as_global_variable_or_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Gvar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::GlobalVariableTargetNode { .. } => {
                let lhs = b.variable(NodeType::Gvar, token(Some(node.location())));
                b.assignable(lhs)
            }
            Node::HashNode { .. } => {
                let n = node.as_hash_node().unwrap();
                let elements = self.visit_all(n.elements().iter());
                self.builder.associate(
                    token(Some(n.opening_loc())),
                    elements,
                    token(Some(n.closing_loc())),
                )
            }
            Node::HashPatternNode { .. } => {
                let n = node.as_hash_pattern_node().unwrap();
                let mut elements: Vec<Node<'_>> = n.elements().iter().collect();
                elements.extend(n.rest());
                let elements = self.visit_all(elements);
                match n.constant() {
                    Some(constant) => {
                        let constant = self.visit(&constant)?;
                        let pattern = self.builder.hash_pattern(None, elements, None);
                        self.builder.const_pattern(
                            constant,
                            token(n.opening_loc()),
                            pattern,
                            token(n.closing_loc()),
                        )
                    }
                    None => self.builder.hash_pattern(
                        token(n.opening_loc()),
                        elements,
                        token(n.closing_loc()),
                    ),
                }
            }
            Node::IfNode { .. } => return self.visit_if_node(node),
            Node::UnlessNode { .. } => return self.visit_unless_node(node),
            Node::InNode { .. } => return self.visit_in_node(node),
            Node::IndexOperatorWriteNode { .. } => {
                let n = node.as_index_operator_write_node().unwrap();
                let receiver = self.visit_opt(n.receiver())?;
                let mut arguments = self.visit_arguments(n.arguments());
                if let Some(block) = n.block() {
                    arguments.extend(self.visit(&block.as_node()));
                }
                let lhs = self.builder.index(
                    receiver,
                    token(Some(n.opening_loc())),
                    arguments,
                    token(Some(n.closing_loc())),
                );
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::IndexAndWriteNode { .. } => {
                let n = node.as_index_and_write_node().unwrap();
                let receiver = self.visit_opt(n.receiver())?;
                let mut arguments = self.visit_arguments(n.arguments());
                if let Some(block) = n.block() {
                    arguments.extend(self.visit(&block.as_node()));
                }
                let lhs = self.builder.index(
                    receiver,
                    token(Some(n.opening_loc())),
                    arguments,
                    token(Some(n.closing_loc())),
                );
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::IndexOrWriteNode { .. } => {
                let n = node.as_index_or_write_node().unwrap();
                let receiver = self.visit_opt(n.receiver())?;
                let mut arguments = self.visit_arguments(n.arguments());
                if let Some(block) = n.block() {
                    arguments.extend(self.visit(&block.as_node()));
                }
                let lhs = self.builder.index(
                    receiver,
                    token(Some(n.opening_loc())),
                    arguments,
                    token(Some(n.closing_loc())),
                );
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::IndexTargetNode { .. } => {
                let n = node.as_index_target_node().unwrap();
                let receiver = self.visit(&n.receiver())?;
                let arguments = self.visit_arguments(n.arguments());
                self.builder.index_asgn(
                    receiver,
                    token(Some(n.opening_loc())),
                    arguments,
                    token(Some(n.closing_loc())),
                )
            }
            Node::InstanceVariableReadNode { .. } => {
                b.variable(NodeType::Ivar, token(Some(node.location())))
            }
            Node::InstanceVariableWriteNode { .. } => {
                let n = node.as_instance_variable_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Ivar, token(Some(n.name_loc())));
                let lhs = self.builder.assignable(lhs);
                let value = self.visit(&n.value())?;
                self.builder
                    .assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::InstanceVariableOperatorWriteNode { .. } => {
                let n = node.as_instance_variable_operator_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Ivar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::InstanceVariableAndWriteNode { .. } => {
                let n = node.as_instance_variable_and_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Ivar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::InstanceVariableOrWriteNode { .. } => {
                let n = node.as_instance_variable_or_write_node().unwrap();
                let lhs = self
                    .builder
                    .variable(NodeType::Ivar, token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::InstanceVariableTargetNode { .. } => {
                let lhs = b.variable(NodeType::Ivar, token(Some(node.location())));
                b.assignable(lhs)
            }
            Node::InterpolatedRegularExpressionNode { .. } => {
                let n = node.as_interpolated_regular_expression_node().unwrap();
                let parts =
                    self.string_nodes_from_interpolation(n.parts().iter(), Some(n.opening_loc()));
                return Some(self.regexp(n.opening_loc(), parts, n.closing_loc()));
            }
            Node::InterpolatedMatchLastLineNode { .. } => {
                let n = node.as_interpolated_match_last_line_node().unwrap();
                let parts =
                    self.string_nodes_from_interpolation(n.parts().iter(), Some(n.opening_loc()));
                return Some(self.regexp(n.opening_loc(), parts, n.closing_loc()));
            }
            Node::InterpolatedStringNode { .. } => {
                let n = node.as_interpolated_string_node().unwrap();
                if let Some(opening) = n.opening_loc()
                    && opening.as_slice().starts_with(b"<<")
                {
                    let parts = n.parts().iter().map(Part::Node).collect();
                    return Some(self.visit_heredoc(
                        NodeType::Str,
                        opening,
                        n.closing_loc().unwrap(),
                        parts,
                    ));
                }
                let parts = self.string_nodes_from_interpolation(n.parts().iter(), n.opening_loc());
                self.builder
                    .string_compose(token(n.opening_loc()), parts, token(n.closing_loc()))
            }
            Node::InterpolatedSymbolNode { .. } => {
                let n = node.as_interpolated_symbol_node().unwrap();
                let parts = self.string_nodes_from_interpolation(n.parts().iter(), n.opening_loc());
                self.builder
                    .symbol_compose(token(n.opening_loc()), parts, token(n.closing_loc()))
            }
            Node::InterpolatedXStringNode { .. } => {
                let n = node.as_interpolated_x_string_node().unwrap();
                if n.opening_loc().as_slice().starts_with(b"<<") {
                    let parts = n.parts().iter().map(Part::Node).collect();
                    return Some(self.visit_heredoc(
                        NodeType::Xstr,
                        n.opening_loc(),
                        n.closing_loc(),
                        parts,
                    ));
                }
                let parts =
                    self.string_nodes_from_interpolation(n.parts().iter(), Some(n.opening_loc()));
                self.builder.xstring_compose(
                    token(Some(n.opening_loc())),
                    parts,
                    token(Some(n.closing_loc())),
                )
            }
            Node::ItLocalVariableReadNode { .. } => {
                let id = b.ident(tok("it", &node.location()));
                b.node_mut(id).ty = NodeType::Lvar;
                id
            }
            Node::ItParametersNode { .. } => b.itarg(),
            Node::KeywordHashNode { .. } => {
                let n = node.as_keyword_hash_node().unwrap();
                let elements = self.visit_all(n.elements().iter());
                self.builder.associate(None, elements, None)
            }
            Node::KeywordRestParameterNode { .. } => {
                let n = node.as_keyword_rest_parameter_node().unwrap();
                let name = n
                    .name()
                    .zip(n.name_loc())
                    .map(|(name, l)| Tok::new(lossy(name.as_slice()), range(&l)));
                b.kwrestarg(token(Some(n.operator_loc())), name)
            }
            Node::LambdaNode { .. } => {
                let n = node.as_lambda_node().unwrap();
                let call = self.builder.call_lambda(token(Some(n.operator_loc())));
                let args = match n.parameters() {
                    None => self.builder.args(None, Vec::new(), None),
                    Some(parameters) => match parameters {
                        Node::NumberedParametersNode { .. } | Node::ItParametersNode { .. } => {
                            self.visit(&parameters)?
                        }
                        _ => {
                            let p = parameters.as_block_parameters_node().unwrap();
                            let list = self.visit_block_parameters(&p);
                            self.builder
                                .args(token(p.opening_loc()), list, token(p.closing_loc()))
                        }
                    },
                };
                let body = self.visit_opt(n.body());
                self.builder.block(
                    call,
                    token(Some(n.opening_loc())),
                    args,
                    body,
                    token(Some(n.closing_loc())),
                )
            }
            Node::LocalVariableReadNode { .. } => {
                let n = node.as_local_variable_read_node().unwrap();
                let id = b.ident(tok(lossy(n.name().as_slice()), &node.location()));
                b.node_mut(id).ty = NodeType::Lvar;
                id
            }
            Node::LocalVariableWriteNode { .. } => {
                let n = node.as_local_variable_write_node().unwrap();
                let lhs = self.builder.ident(token(Some(n.name_loc())));
                let lhs = self.builder.assignable(lhs);
                let value = self.visit(&n.value())?;
                self.builder
                    .assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::LocalVariableOperatorWriteNode { .. } => {
                let n = node.as_local_variable_operator_write_node().unwrap();
                let lhs = self.builder.ident(token(Some(n.name_loc())));
                return self.op_write(lhs, n.binary_operator_loc(), &n.value());
            }
            Node::LocalVariableAndWriteNode { .. } => {
                let n = node.as_local_variable_and_write_node().unwrap();
                let lhs = self.builder.ident(token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::LocalVariableOrWriteNode { .. } => {
                let n = node.as_local_variable_or_write_node().unwrap();
                let lhs = self.builder.ident(token(Some(n.name_loc())));
                return self.op_write(lhs, n.operator_loc(), &n.value());
            }
            Node::LocalVariableTargetNode { .. } => {
                let n = node.as_local_variable_target_node().unwrap();
                if self.state.in_pattern {
                    let var = b.match_var(tok(lossy(n.name().as_slice()), &node.location()));
                    b.assignable(var)
                } else {
                    let var = b.ident(token(Some(node.location())));
                    b.assignable(var)
                }
            }
            Node::MatchPredicateNode { .. } => {
                let n = node.as_match_predicate_node().unwrap();
                let value = self.visit(&n.value())?;
                let pattern = self.with_pattern(true, |c| c.visit(&n.pattern()))?;
                self.builder.match_pattern(
                    NodeType::MatchPatternP,
                    value,
                    token(Some(n.keyword_loc())),
                    pattern,
                )
            }
            Node::MatchRequiredNode { .. } => {
                let n = node.as_match_required_node().unwrap();
                let value = self.visit(&n.value())?;
                let pattern = self.with_pattern(true, |c| c.visit(&n.pattern()))?;
                self.builder.match_pattern(
                    NodeType::MatchPattern,
                    value,
                    token(Some(n.operator_loc())),
                    pattern,
                )
            }
            Node::MatchWriteNode { .. } => {
                let n = node.as_match_write_node().unwrap();
                let call = n.call();
                let receiver = self.visit_opt(call.receiver())?;
                let arg = call.arguments().and_then(|a| a.arguments().iter().next());
                let arg = self.visit_opt(arg)?;
                self.builder
                    .match_op(receiver, token(call.message_loc()), arg)
            }
            Node::ErrorRecoveryNode { .. } => return None,
            Node::MultiTargetNode { .. } => {
                let n = node.as_multi_target_node().unwrap();
                let mut elements: Vec<Node<'_>> = n.lefts().iter().collect();
                if let Some(rest) = n.rest()
                    && !matches!(rest, Node::ImplicitRestNode { .. })
                {
                    elements.push(rest);
                }
                elements.extend(n.rights().iter());
                let items = self.visit_all(elements);
                self.builder
                    .multi_lhs(token(n.lparen_loc()), items, token(n.rparen_loc()))
            }
            Node::MultiWriteNode { .. } => {
                let n = node.as_multi_write_node().unwrap();
                let mut elements: Vec<Node<'_>> = n.lefts().iter().collect();
                let rest = n
                    .rest()
                    .filter(|rest| !matches!(rest, Node::ImplicitRestNode { .. }));
                elements.extend(rest);
                elements.extend(n.rights().iter());
                if elements.len() == 1
                    && n.rest().is_none()
                    && let Some(inner) = elements[0].as_multi_target_node()
                {
                    let mut inner_elements: Vec<Node<'_>> = inner.lefts().iter().collect();
                    if let Some(rest) = inner.rest()
                        && !matches!(rest, Node::ImplicitRestNode { .. })
                    {
                        inner_elements.push(rest);
                    }
                    inner_elements.extend(inner.rights().iter());
                    elements = inner_elements;
                }
                let items = self.visit_all(elements);
                let lhs =
                    self.builder
                        .multi_lhs(token(n.lparen_loc()), items, token(n.rparen_loc()));
                let value = self.visit(&n.value())?;
                self.builder
                    .multi_assign(lhs, token(Some(n.operator_loc())), value)
            }
            Node::NoKeywordsParameterNode { .. } => {
                let n = node.as_no_keywords_parameter_node().unwrap();
                if self.state.in_pattern {
                    b.match_nil_pattern(token(Some(n.operator_loc())), token(Some(n.keyword_loc())))
                } else {
                    b.kwnilarg(token(Some(n.operator_loc())), token(Some(n.keyword_loc())))
                }
            }
            Node::NumberedParametersNode { .. } => {
                let n = node.as_numbered_parameters_node().unwrap();
                b.numargs(i64::from(n.maximum()))
            }
            Node::NumberedReferenceReadNode { .. } => {
                let n = node.as_numbered_reference_read_node().unwrap();
                b.nth_ref(i64::from(n.number()), range(&node.location()))
            }
            Node::OptionalKeywordParameterNode { .. } => {
                let n = node.as_optional_keyword_parameter_node().unwrap();
                let value = self.visit(&n.value())?;
                let name = Tok::new(lossy(n.name().as_slice()), range(&n.name_loc()));
                self.builder.kwoptarg(Some(name), value)
            }
            Node::OptionalParameterNode { .. } => {
                let n = node.as_optional_parameter_node().unwrap();
                let value = self.visit(&n.value())?;
                self.builder.optarg(
                    token(Some(n.name_loc())),
                    token(Some(n.operator_loc())),
                    value,
                )
            }
            Node::ParenthesesNode { .. } => {
                let n = node.as_parentheses_node().unwrap();
                let body = self.visit_opt(n.body());
                self.builder.begin(
                    token(Some(n.opening_loc())),
                    body,
                    token(Some(n.closing_loc())),
                )
            }
            Node::PinnedExpressionNode { .. } => {
                let n = node.as_pinned_expression_node().unwrap();
                let parts = self.with_pattern(false, |c| c.visit(&n.expression()));
                let expression = self.builder.begin(
                    token(Some(n.lparen_loc())),
                    parts,
                    token(Some(n.rparen_loc())),
                );
                self.builder.pin(token(Some(n.operator_loc())), expression)
            }
            Node::PinnedVariableNode { .. } => {
                let n = node.as_pinned_variable_node().unwrap();
                let var = self.visit(&n.variable())?;
                self.builder.pin(token(Some(n.operator_loc())), var)
            }
            Node::PostExecutionNode { .. } => {
                let n = node.as_post_execution_node().unwrap();
                let body = n.statements().and_then(|s| self.visit(&s.as_node()));
                self.builder.preexe(
                    NodeType::Postexe,
                    token(Some(n.keyword_loc())),
                    token(Some(n.opening_loc())),
                    body,
                    token(Some(n.closing_loc())),
                )
            }
            Node::PreExecutionNode { .. } => {
                let n = node.as_pre_execution_node().unwrap();
                let body = n.statements().and_then(|s| self.visit(&s.as_node()));
                self.builder.preexe(
                    NodeType::Preexe,
                    token(Some(n.keyword_loc())),
                    token(Some(n.opening_loc())),
                    body,
                    token(Some(n.closing_loc())),
                )
            }
            Node::ProgramNode { .. } => {
                let n = node.as_program_node().unwrap();
                return self.visit(&n.statements().as_node());
            }
            Node::RangeNode { .. } => {
                let n = node.as_range_node().unwrap();
                let left = self.visit_opt(n.left());
                let right = self.visit_opt(n.right());
                let ty = if n.is_exclude_end() {
                    NodeType::Erange
                } else {
                    NodeType::Irange
                };
                self.builder
                    .range(ty, left, token(Some(n.operator_loc())), right)
            }
            Node::FlipFlopNode { .. } => {
                let n = node.as_flip_flop_node().unwrap();
                let left = self.visit_opt(n.left());
                let right = self.visit_opt(n.right());
                let ty = if n.is_exclude_end() {
                    NodeType::Erange
                } else {
                    NodeType::Irange
                };
                self.builder
                    .range(ty, left, token(Some(n.operator_loc())), right)
            }
            Node::RedoNode { .. } => b.keyword_cmd(
                NodeType::Redo,
                token(Some(node.location())),
                None,
                Vec::new(),
                None,
            ),
            Node::RetryNode { .. } => b.keyword_cmd(
                NodeType::Retry,
                token(Some(node.location())),
                None,
                Vec::new(),
                None,
            ),
            Node::RegularExpressionNode { .. } => {
                let n = node.as_regular_expression_node().unwrap();
                let content = n.content_loc();
                let parts = self.literal_parts(n.unescaped(), content, Some(n.opening_loc()), true);
                return Some(self.regexp(n.opening_loc(), parts, n.closing_loc()));
            }
            Node::MatchLastLineNode { .. } => {
                let n = node.as_match_last_line_node().unwrap();
                let content = n.content_loc();
                let parts = self.literal_parts(n.unescaped(), content, Some(n.opening_loc()), true);
                return Some(self.regexp(n.opening_loc(), parts, n.closing_loc()));
            }
            Node::RequiredKeywordParameterNode { .. } => {
                let n = node.as_required_keyword_parameter_node().unwrap();
                b.kwarg(Some(Tok::new(
                    lossy(n.name().as_slice()),
                    range(&n.name_loc()),
                )))
            }
            Node::RequiredParameterNode { .. } => b.arg(token(Some(node.location()))),
            Node::RescueModifierNode { .. } => {
                let n = node.as_rescue_modifier_node().unwrap();
                let expression = self.visit(&n.expression());
                let rescue_expression = self.visit(&n.rescue_expression());
                let body = self.builder.rescue_body(
                    token(Some(n.keyword_loc())),
                    None,
                    None,
                    None,
                    None,
                    rescue_expression,
                );
                return self
                    .builder
                    .begin_body(expression, vec![body], None, None, None, None);
            }
            Node::RestParameterNode { .. } => {
                let n = node.as_rest_parameter_node().unwrap();
                b.restarg(token(Some(n.operator_loc())), token(n.name_loc()))
            }
            Node::ShareableConstantNode { .. } => {
                let n = node.as_shareable_constant_node().unwrap();
                return self.visit(&n.write());
            }
            Node::SourceEncodingNode { .. } => {
                let id = b.encoding(token(Some(node.location())));
                b.accessible(id)
            }
            Node::SourceFileNode { .. } => {
                let id = b.file(token(Some(node.location())));
                b.accessible(id)
            }
            Node::SourceLineNode { .. } => {
                let id = b.line(token(Some(node.location())));
                b.accessible(id)
            }
            Node::SplatNode { .. } => {
                let n = node.as_splat_node().unwrap();
                let op = token(Some(n.operator_loc()));
                if n.expression().is_none() && self.state.forwarding.contains(&"*") {
                    self.builder.forwarded_restarg(op)
                } else if self.state.in_destructure {
                    let name = n.expression().map(|e| e.location());
                    self.builder.restarg(op, token(name))
                } else if self.state.in_pattern {
                    let name = n.expression().map(|e| e.location());
                    self.builder.match_rest(op, token(name))
                } else {
                    let expression = self.visit_opt(n.expression());
                    self.builder.splat(op, expression)
                }
            }
            Node::StatementsNode { .. } => {
                let n = node.as_statements_node().unwrap();
                let statements = self.visit_all(n.body().iter());
                return self.builder.compstmt(statements);
            }
            Node::StringNode { .. } => return Some(self.visit_string_node(node)),
            Node::SuperNode { .. } => {
                let n = node.as_super_node().unwrap();
                let mut arguments = self.visit_arguments(n.arguments());
                let mut block = n.block();
                if let Some(Node::BlockArgumentNode { .. }) = block {
                    arguments.extend(self.visit_opt(block));
                    block = None;
                }
                let call = self.builder.keyword_cmd(
                    NodeType::Super,
                    token(Some(n.keyword_loc())),
                    token(n.lparen_loc()),
                    arguments,
                    token(n.rparen_loc()),
                );
                let block = block.and_then(|b| b.as_block_node());
                return Some(self.visit_block(call, block));
            }
            Node::SymbolNode { .. } => return Some(self.visit_symbol_node(node)),
            Node::UndefNode { .. } => {
                let n = node.as_undef_node().unwrap();
                let names = self.visit_all(n.names().iter());
                self.builder
                    .undef_method(token(Some(n.keyword_loc())), names)
            }
            Node::UntilNode { .. } => {
                let n = node.as_until_node().unwrap();
                return self.visit_loop(
                    NodeType::Until,
                    node,
                    n.until_keyword_loc(),
                    n.do_keyword_loc(),
                    n.end_keyword_loc(),
                    n.predicate(),
                    n.statements(),
                );
            }
            Node::WhileNode { .. } => {
                let n = node.as_while_node().unwrap();
                return self.visit_loop(
                    NodeType::While,
                    node,
                    n.while_keyword_loc(),
                    n.do_keyword_loc(),
                    n.end_keyword_loc(),
                    n.predicate(),
                    n.statements(),
                );
            }
            Node::WhenNode { .. } => {
                let n = node.as_when_node().unwrap();
                let conditions: Vec<Node<'_>> = n.conditions().iter().collect();
                let then_t = match n.then_keyword_loc() {
                    Some(then) => token(Some(then)),
                    None => self.srange_semicolon(
                        conditions.last().unwrap().location().end() as usize,
                        n.statements().map(|s| s.location().start() as usize),
                    ),
                };
                let patterns = self.visit_all(conditions);
                let body = n.statements().and_then(|s| self.visit(&s.as_node()));
                self.builder
                    .when(token(Some(n.when_keyword_loc())), patterns, then_t, body)
            }
            Node::XStringNode { .. } => {
                let n = node.as_x_string_node().unwrap();
                if n.opening_loc().as_slice().starts_with(b"<<") {
                    let part = Part::Str {
                        unescaped: n.unescaped(),
                        content: n.content_loc().as_slice(),
                        location: n.content_loc(),
                    };
                    return Some(self.visit_heredoc(
                        NodeType::Xstr,
                        n.opening_loc(),
                        n.closing_loc(),
                        vec![part],
                    ));
                }
                let parts =
                    self.literal_parts(n.unescaped(), n.content_loc(), Some(n.opening_loc()), true);
                self.builder.xstring_compose(
                    token(Some(n.opening_loc())),
                    parts,
                    token(Some(n.closing_loc())),
                )
            }
            Node::YieldNode { .. } => {
                let n = node.as_yield_node().unwrap();
                let args = self.visit_arguments(n.arguments());
                self.builder.keyword_cmd(
                    NodeType::Yield,
                    token(Some(n.keyword_loc())),
                    token(n.lparen_loc()),
                    args,
                    token(n.rparen_loc()),
                )
            }
            Node::ParametersNode { .. }
            | Node::BlockParametersNode { .. }
            | Node::ArgumentsNode { .. }
            | Node::BlockNode { .. }
            | Node::EnsureNode { .. }
            | Node::RescueNode { .. }
            | Node::ImplicitNode { .. }
            | Node::ImplicitRestNode { .. } => {
                return None;
            }
        })
    }

    // ---- individual visitors ------------------------------------------------------

    fn op_write(
        &mut self,
        lhs: NodeId,
        operator: Location<'_>,
        value: &Node<'_>,
    ) -> Option<NodeId> {
        let lhs = self.builder.assignable(lhs);
        let value = self.visit(value)?;
        let op = tok(lossy(chomp_eq(operator.as_slice())), &operator);
        Some(self.builder.op_assign(lhs, op, value))
    }

    fn constant_path(
        &mut self,
        parent: Option<Node<'_>>,
        delimiter: Location<'_>,
        name: Location<'_>,
    ) -> Option<NodeId> {
        let name_t = token(Some(name));
        Some(match parent {
            None => self.builder.const_global(token(Some(delimiter)), name_t),
            Some(parent) => {
                let scope = self.visit(&parent)?;
                self.builder
                    .const_fetch(scope, token(Some(delimiter)), name_t)
            }
        })
    }

    fn numeric_value(node: &Node<'_>) -> Child {
        match node {
            Node::IntegerNode { .. } => integer_value(&node.as_integer_node().unwrap().value()),
            Node::FloatNode { .. } => Child::Float(node.as_float_node().unwrap().value()),
            Node::RationalNode { .. } => {
                let n = node.as_rational_node().unwrap();
                Child::Rational(
                    integer_string(&n.numerator()),
                    integer_string(&n.denominator()),
                )
            }
            Node::ImaginaryNode { .. } => Child::Complex(Box::new(Self::numeric_value(
                &node.as_imaginary_node().unwrap().numeric(),
            ))),
            _ => Child::Nil,
        }
    }

    /// `visit_numeric`: a leading sign becomes the node's `operator`.
    fn visit_numeric(&mut self, ty: NodeType, value: Child, location: Range) -> NodeId {
        let numeric = self.builder.numeric(ty, value, location);
        let first = self.source()[location.begin_pos];
        if first == b'+' || first == b'-' {
            let sign = Tok::new(
                (first as char).to_string(),
                Range::new(location.begin_pos, location.begin_pos + 1),
            );
            self.builder.unary_num(Some(sign), numeric)
        } else {
            numeric
        }
    }

    fn visit_array_node(&mut self, node: &Node<'_>) -> NodeId {
        let n = node.as_array_node().unwrap();
        let opening = n.opening_loc();
        let percent = opening.as_ref().is_some_and(|o| {
            let s = o.as_slice();
            s.starts_with(b"%w")
                || s.starts_with(b"%W")
                || s.starts_with(b"%i")
                || s.starts_with(b"%I")
        });
        let elements = if percent {
            let mut elements = Vec::new();
            for element in n.elements().iter() {
                match element {
                    Node::StringNode { .. } => {
                        let s = element.as_string_node().unwrap();
                        let content = s.content_loc();
                        if content.as_slice().contains(&b'\n') {
                            elements.extend(self.string_nodes_from_line_continuations(
                                s.unescaped(),
                                content.as_slice(),
                                content.start() as usize,
                                opening.as_ref().map(Location::as_slice),
                            ));
                        } else {
                            elements.push(
                                self.builder
                                    .string_internal_bytes(s.unescaped(), range(&content)),
                            );
                        }
                    }
                    Node::InterpolatedStringNode { .. } => {
                        let s = element.as_interpolated_string_node().unwrap();
                        let parts =
                            self.string_nodes_from_interpolation(s.parts().iter(), n.opening_loc());
                        elements.push(self.builder.string_compose(
                            token(s.opening_loc()),
                            parts,
                            token(s.closing_loc()),
                        ));
                    }
                    _ => elements.extend(self.visit(&element)),
                }
            }
            elements
        } else {
            self.visit_all(n.elements().iter())
        };
        self.builder
            .array(token(n.opening_loc()), elements, token(n.closing_loc()))
    }

    fn visit_array_pattern_node(&mut self, node: &Node<'_>) -> NodeId {
        let n = node.as_array_pattern_node().unwrap();
        let mut elements: Vec<Node<'_>> = n.requireds().iter().collect();
        let rest = n.rest();
        let implicit_rest = rest
            .as_ref()
            .is_some_and(|r| matches!(r, Node::ImplicitRestNode { .. }));
        if !implicit_rest && let Some(rest) = n.rest() {
            elements.push(rest);
        }
        elements.extend(n.posts().iter());
        let mut visited = self.visit_all(elements);
        if implicit_rest && let Some(last) = visited.pop() {
            let comma = token(rest.map(|r| r.location()));
            visited.push(self.builder.match_with_trailing_comma(last, comma));
        }
        match n.constant() {
            Some(constant) => {
                let constant = self.visit(&constant).unwrap();
                let pattern = if visited.is_empty() {
                    self.builder.array_pattern(
                        token(n.opening_loc()),
                        visited,
                        token(n.closing_loc()),
                    )
                } else {
                    self.builder.array_pattern(None, visited, None)
                };
                self.builder.const_pattern(
                    constant,
                    token(n.opening_loc()),
                    pattern,
                    token(n.closing_loc()),
                )
            }
            None => {
                self.builder
                    .array_pattern(token(n.opening_loc()), visited, token(n.closing_loc()))
            }
        }
    }

    fn visit_assoc_node(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let n = node.as_assoc_node().unwrap();
        let key = n.key();
        let value = n.value();
        if let Some(implicit) = value.as_implicit_node() {
            if self.state.in_pattern {
                if let Some(sym) = key.as_symbol_node() {
                    if sym.opening_loc().is_none() {
                        return Some(self.builder.match_hash_var(Some(Tok::new(
                            lossy(sym.unescaped()),
                            range(&key.location()),
                        ))));
                    }
                    let value_loc = Some(sym.content_loc()).unwrap();
                    let s = self
                        .builder
                        .string_internal_bytes(sym.unescaped(), range(&value_loc));
                    return Some(self.builder.match_hash_var_from_str(
                        token(sym.opening_loc()),
                        vec![s],
                        token(sym.closing_loc()),
                    ));
                }
                let sym = key.as_interpolated_symbol_node().unwrap();
                let parts = self.visit_all(sym.parts().iter());
                return Some(self.builder.match_hash_var_from_str(
                    token(sym.opening_loc()),
                    parts,
                    token(sym.closing_loc()),
                ));
            }
            let sym = key.as_symbol_node().unwrap();
            let value_loc = Some(sym.content_loc()).unwrap();
            let inner = implicit.value();
            let implicit_value = match inner {
                Node::CallNode { .. } => {
                    let call = inner.as_call_node().unwrap();
                    self.builder.call_method(
                        None,
                        None,
                        Some(Tok::new(
                            lossy(call.name().as_slice()),
                            range(&call.message_loc().unwrap()),
                        )),
                        None,
                        Vec::new(),
                        None,
                    )
                }
                Node::ConstantReadNode { .. } => {
                    let name = inner.as_constant_read_node().unwrap().name();
                    self.builder
                        .const_(Some(Tok::new(lossy(name.as_slice()), range(&value_loc))))
                }
                _ => {
                    let name = match inner {
                        Node::LocalVariableReadNode { .. } => lossy(
                            inner
                                .as_local_variable_read_node()
                                .unwrap()
                                .name()
                                .as_slice(),
                        ),
                        _ => lossy(value_loc.as_slice()),
                    };
                    let id = self.builder.ident(Some(Tok::new(name, range(&value_loc))));
                    self.builder.node_mut(id).ty = NodeType::Lvar;
                    id
                }
            };
            return Some(self.builder.pair_keyword(
                Some(Tok::new(lossy(sym.unescaped()), range(&key.location()))),
                implicit_value,
            ));
        }
        if let Some(operator) = n.operator_loc() {
            let key = self.visit(&key)?;
            let value = self.visit(&value)?;
            return Some(self.builder.pair(key, token(Some(operator)), value));
        }
        if let Some(sym) = key.as_symbol_node()
            && sym.opening_loc().is_none()
        {
            let value = self.visit(&value)?;
            return Some(self.builder.pair_keyword(
                Some(Tok::new(lossy(sym.unescaped()), range(&key.location()))),
                value,
            ));
        }
        let (opening, parts, closing) = if let Some(sym) = key.as_symbol_node() {
            let s = self
                .builder
                .string_internal_bytes(sym.unescaped(), range(&Some(sym.content_loc()).unwrap()));
            (sym.opening_loc(), vec![s], sym.closing_loc())
        } else {
            let sym = key.as_interpolated_symbol_node().unwrap();
            let parts = self.visit_all(sym.parts().iter());
            (sym.opening_loc(), parts, sym.closing_loc())
        };
        let value = self.visit(&value)?;
        Some(
            self.builder
                .pair_quoted(token(opening), parts, token(closing), value),
        )
    }

    fn visit_begin_node(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let n = node.as_begin_node().unwrap();
        let mut rescue_bodies = Vec::new();
        let mut rescue_clause = n.rescue_clause();
        while let Some(clause) = rescue_clause {
            let exceptions: Vec<Node<'_>> = clause.exceptions().iter().collect();
            let find_start_offset = clause
                .reference()
                .map(|r| r.location())
                .or_else(|| exceptions.last().map(Node::location))
                .unwrap_or_else(|| clause.keyword_loc())
                .end() as usize;
            let find_end_offset = clause
                .statements()
                .map(|s| s.location().start() as usize)
                .or_else(|| clause.subsequent().map(|s| s.location().start() as usize))
                .or_else(|| n.else_clause().map(|e| e.location().start() as usize))
                .or_else(|| n.ensure_clause().map(|e| e.location().start() as usize))
                .or_else(|| n.end_keyword_loc().map(|e| e.start() as usize))
                .unwrap_or(find_start_offset + 1);

            let exc_list = if exceptions.is_empty() {
                None
            } else {
                let list = self.visit_all(exceptions);
                Some(self.builder.array(None, list, None))
            };
            let reference = self.visit_opt(clause.reference());
            let then_t = self.srange_semicolon(find_start_offset, Some(find_end_offset));
            let statements = clause.statements().and_then(|s| self.visit(&s.as_node()));
            rescue_bodies.push(self.builder.rescue_body(
                token(Some(clause.keyword_loc())),
                exc_list,
                token(clause.operator_loc()),
                reference,
                then_t,
                statements,
            ));
            rescue_clause = clause.subsequent();
        }

        let statements = n.statements().and_then(|s| self.visit(&s.as_node()));
        let else_clause = n.else_clause();
        let else_t = token(else_clause.as_ref().map(|e| e.else_keyword_loc()));
        let else_body = else_clause
            .as_ref()
            .and_then(|e| e.statements())
            .and_then(|s| self.visit(&s.as_node()));
        let ensure_clause = n.ensure_clause();
        let ensure_t = token(ensure_clause.as_ref().map(|e| e.ensure_keyword_loc()));
        let ensure_body = ensure_clause
            .as_ref()
            .and_then(|e| e.statements())
            .and_then(|s| self.visit(&s.as_node()));
        let begin_body = self.builder.begin_body(
            statements,
            rescue_bodies,
            else_t,
            else_body,
            ensure_t,
            ensure_body,
        );

        match n.begin_keyword_loc() {
            Some(begin) => Some(self.builder.begin_keyword(
                token(Some(begin)),
                begin_body,
                token(n.end_keyword_loc()),
            )),
            None => begin_body,
        }
    }

    fn visit_call_node(&mut self, node: &Node<'_>) -> NodeId {
        let n = node.as_call_node().unwrap();
        let name = n.name().as_slice();
        let mut arguments: Vec<Node<'_>> = n
            .arguments()
            .map(|a| a.arguments().iter().collect())
            .unwrap_or_default();
        let mut block = n.block();
        if let Some(Node::BlockArgumentNode { .. }) = block {
            arguments.push(block.take().unwrap());
        }
        let block = block.and_then(|b| b.as_block_node());

        if n.call_operator_loc().is_none() {
            match name {
                b"-@" => {
                    let receiver = n.receiver().unwrap();
                    if matches!(
                        receiver,
                        Node::IntegerNode { .. }
                            | Node::FloatNode { .. }
                            | Node::RationalNode { .. }
                            | Node::ImaginaryNode { .. }
                    ) {
                        let location = join(
                            range(&n.message_loc().unwrap()),
                            range(&receiver.location()),
                        );
                        let (ty, value) = match receiver {
                            Node::IntegerNode { .. } => {
                                (NodeType::Int, negate(Self::numeric_value(&receiver)))
                            }
                            Node::FloatNode { .. } => {
                                (NodeType::Float, negate(Self::numeric_value(&receiver)))
                            }
                            Node::RationalNode { .. } => {
                                (NodeType::Rational, negate(Self::numeric_value(&receiver)))
                            }
                            _ => (NodeType::Complex, negate(Self::numeric_value(&receiver))),
                        };
                        return self.visit_numeric(ty, value, location);
                    }
                }
                b"!" => {
                    let receiver = self.visit_opt(n.receiver());
                    let call = self.builder.not_op(
                        token(n.message_loc()),
                        token(n.opening_loc()),
                        receiver,
                        token(n.closing_loc()),
                    );
                    return self.visit_block(call, block);
                }
                b"=~" => {
                    if let Some(receiver) = n.receiver()
                        && matches!(receiver, Node::RegularExpressionNode { .. })
                    {
                        let receiver = self.visit(&receiver).unwrap();
                        let arg = n
                            .arguments()
                            .and_then(|a| a.arguments().iter().next())
                            .and_then(|a| self.visit(&a))
                            .unwrap();
                        return self.builder.match_op(receiver, token(n.message_loc()), arg);
                    }
                }
                b"[]" => {
                    let receiver = self.visit_opt(n.receiver()).unwrap();
                    let args = self.visit_all(arguments);
                    let call = self.builder.index(
                        receiver,
                        token(n.opening_loc()),
                        args,
                        token(n.closing_loc()),
                    );
                    return self.visit_block(call, block);
                }
                b"[]=" => {
                    let message_is_name = n.message_loc().is_some_and(|m| m.as_slice() == b"[]=");
                    if !message_is_name
                        && let Some(args_node) = n.arguments()
                        && block.is_none()
                        && !n.is_safe_navigation()
                    {
                        let mut all: Vec<Node<'_>> = args_node.arguments().iter().collect();
                        let last = all.pop().unwrap();
                        if let Some(block_arg) = n.block() {
                            all.push(block_arg);
                        }
                        let receiver = self.visit_opt(n.receiver()).unwrap();
                        let args = self.visit_all(all);
                        let lhs = self.builder.index_asgn(
                            receiver,
                            token(n.opening_loc()),
                            args,
                            token(n.closing_loc()),
                        );
                        let value = self.visit(&last).unwrap();
                        let call = self.builder.assign(lhs, token(n.equal_loc()), value);
                        return self.visit_block(call, block);
                    }
                }
                _ => {}
            }
        }

        let message_loc = n.message_loc();
        let call_operator = Self::call_operator(n.call_operator_loc());
        let receiver = self.visit_opt(n.receiver());
        let call = if name.ends_with(b"=")
            && message_loc
                .as_ref()
                .is_some_and(|m| !m.as_slice().ends_with(b"="))
            && n.arguments().is_some()
            && block.is_none()
        {
            let lhs = self
                .builder
                .attr_asgn(receiver, call_operator, token(message_loc));
            let value = n
                .arguments()
                .and_then(|a| a.arguments().iter().last())
                .and_then(|a| self.visit(&a))
                .unwrap();
            self.builder.assign(lhs, token(n.equal_loc()), value)
        } else {
            let selector = message_loc.map(|m| Tok::new(lossy(name), range(&m)));
            let args = self.visit_all(arguments);
            self.builder.call_method(
                receiver,
                call_operator,
                selector,
                token(n.opening_loc()),
                args,
                token(n.closing_loc()),
            )
        };
        self.visit_block(call, block)
    }

    fn visit_def_node(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let n = node.as_def_node().unwrap();
        let forwarding = find_forwarding(n.parameters());
        let params = match n.parameters() {
            Some(p) => self.visit_parameters(&p),
            None => Vec::new(),
        };
        let args = self
            .builder
            .args(token(n.lparen_loc()), params, token(n.rparen_loc()));
        let receiver = n.receiver().map(|r| match r.as_parentheses_node() {
            Some(paren) => paren.body(),
            None => Some(r),
        });
        let definee = match receiver {
            Some(Some(r)) => self.visit(&r),
            _ => None,
        };
        let body = self.with_forwarding(forwarding, |c| c.visit_opt(n.body()));
        let name_t = token(Some(n.name_loc()));
        let def_t = token(Some(n.def_keyword_loc()));
        Some(match (n.equal_loc(), definee) {
            (Some(equal), Some(definee)) => self.builder.def_endless_singleton(
                def_t,
                definee,
                token(n.operator_loc()),
                name_t,
                args,
                token(Some(equal)),
                body,
            ),
            (Some(equal), None) => {
                self.builder
                    .def_endless_method(def_t, name_t, args, token(Some(equal)), body)
            }
            (None, Some(definee)) => self.builder.def_singleton(
                def_t,
                definee,
                token(n.operator_loc()),
                name_t,
                args,
                body,
                token(n.end_keyword_loc()),
            ),
            (None, None) => {
                self.builder
                    .def_method(def_t, name_t, args, body, token(n.end_keyword_loc()))
            }
        })
    }

    fn visit_if_node(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let n = node.as_if_node().unwrap();
        let predicate = n.predicate();
        match n.if_keyword_loc() {
            None => {
                let cond = self.visit(&predicate)?;
                let if_true = n.statements().and_then(|s| self.visit(&s.as_node()))?;
                let subsequent = n.subsequent().unwrap();
                let else_node = subsequent.as_else_node().unwrap();
                let if_false = self.visit(&subsequent)?;
                Some(self.builder.ternary(
                    cond,
                    token(n.then_keyword_loc()),
                    if_true,
                    token(Some(else_node.else_keyword_loc())),
                    if_false,
                ))
            }
            Some(if_keyword) if if_keyword.start() as usize == node.location().start() as usize => {
                let then_t = match n.then_keyword_loc() {
                    Some(then) => token(Some(then)),
                    None => {
                        let end = n
                            .statements()
                            .map(|s| s.location().start() as usize)
                            .or_else(|| n.subsequent().map(|s| s.location().start() as usize))
                            .or_else(|| n.end_keyword_loc().map(|e| e.start() as usize));
                        self.srange_semicolon(predicate.location().end() as usize, end)
                    }
                };
                let cond = self.visit(&predicate)?;
                let if_true = n.statements().and_then(|s| self.visit(&s.as_node()));
                let else_t = n.subsequent().and_then(|s| match s {
                    Node::IfNode { .. } => s.as_if_node().unwrap().if_keyword_loc(),
                    Node::ElseNode { .. } => Some(s.as_else_node().unwrap().else_keyword_loc()),
                    _ => None,
                });
                let if_false = self.visit_opt(n.subsequent());
                let end_t = if if_keyword.as_slice() != b"elsif" {
                    token(n.end_keyword_loc())
                } else {
                    None
                };
                Some(self.builder.condition(
                    token(Some(if_keyword)),
                    cond,
                    then_t,
                    if_true,
                    token(else_t),
                    if_false,
                    end_t,
                ))
            }
            Some(if_keyword) => {
                let if_true = n.statements().and_then(|s| self.visit(&s.as_node()));
                let if_false = self.visit_opt(n.subsequent());
                let cond = self.visit(&predicate)?;
                Some(
                    self.builder
                        .condition_mod(if_true, if_false, token(Some(if_keyword)), cond),
                )
            }
        }
    }

    fn visit_unless_node(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let n = node.as_unless_node().unwrap();
        let predicate = n.predicate();
        if n.unless_keyword_loc().start() as usize == node.location().start() as usize {
            let then_t = match n.then_keyword_loc() {
                Some(then) => token(Some(then)),
                None => {
                    let end = n
                        .statements()
                        .map(|s| s.location().start() as usize)
                        .or_else(|| n.else_clause().map(|s| s.location().start() as usize))
                        .or_else(|| n.end_keyword_loc().map(|e| e.start() as usize));
                    self.srange_semicolon(predicate.location().end() as usize, end)
                }
            };
            let cond = self.visit(&predicate)?;
            let else_clause = n.else_clause();
            let if_true = else_clause
                .as_ref()
                .and_then(|e| e.statements())
                .and_then(|s| self.visit(&s.as_node()));
            let else_t = token(else_clause.as_ref().map(|e| e.else_keyword_loc()));
            let if_false = n.statements().and_then(|s| self.visit(&s.as_node()));
            Some(self.builder.condition(
                token(Some(n.unless_keyword_loc())),
                cond,
                then_t,
                if_true,
                else_t,
                if_false,
                token(n.end_keyword_loc()),
            ))
        } else {
            let if_true = n
                .else_clause()
                .and_then(|e| e.statements())
                .and_then(|s| self.visit(&s.as_node()));
            let if_false = n.statements().and_then(|s| self.visit(&s.as_node()));
            let cond = self.visit(&predicate)?;
            Some(self.builder.condition_mod(
                if_true,
                if_false,
                token(Some(n.unless_keyword_loc())),
                cond,
            ))
        }
    }

    fn visit_in_node(&mut self, node: &Node<'_>) -> Option<NodeId> {
        let n = node.as_in_node().unwrap();
        let pattern_node = n.pattern();
        let (pattern, guard) = match pattern_node {
            Node::IfNode { .. } => {
                let p = pattern_node.as_if_node().unwrap();
                let pattern = self
                    .with_pattern(true, |c| p.statements().and_then(|s| c.visit(&s.as_node())))?;
                let predicate = self.visit(&p.predicate())?;
                let guard =
                    self.builder
                        .guard(NodeType::IfGuard, token(p.if_keyword_loc()), predicate);
                (pattern, Some(guard))
            }
            Node::UnlessNode { .. } => {
                let p = pattern_node.as_unless_node().unwrap();
                let pattern = self
                    .with_pattern(true, |c| p.statements().and_then(|s| c.visit(&s.as_node())))?;
                let predicate = self.visit(&p.predicate())?;
                let guard = self.builder.guard(
                    NodeType::UnlessGuard,
                    token(Some(p.unless_keyword_loc())),
                    predicate,
                );
                (pattern, Some(guard))
            }
            _ => (self.with_pattern(true, |c| c.visit(&pattern_node))?, None),
        };
        let then_t = match n.then_keyword_loc() {
            Some(then) => token(Some(then)),
            None => self.srange_semicolon(
                pattern_node.location().end() as usize,
                n.statements().map(|s| s.location().start() as usize),
            ),
        };
        let body = n.statements().and_then(|s| self.visit(&s.as_node()));
        Some(self.builder.in_pattern(
            token(Some(n.in_keyword_loc())),
            pattern,
            guard,
            then_t,
            body,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_loop(
        &mut self,
        ty: NodeType,
        node: &Node<'_>,
        keyword: Location<'_>,
        do_keyword: Option<Location<'_>>,
        closing: Option<Location<'_>>,
        predicate: Node<'_>,
        statements: Option<ruby_prism::StatementsNode<'_>>,
    ) -> Option<NodeId> {
        if node.location().start() as usize == keyword.start() as usize {
            let do_t = match do_keyword {
                Some(do_loc) => token(Some(do_loc)),
                None => {
                    let end = statements
                        .as_ref()
                        .map(|s| s.location().start() as usize)
                        .or_else(|| closing.as_ref().map(|l| l.start() as usize));
                    self.srange_semicolon(predicate.location().end() as usize, end)
                }
            };
            let cond = self.visit(&predicate)?;
            let body = statements.and_then(|s| self.visit(&s.as_node()));
            Some(
                self.builder
                    .loop_(ty, token(Some(keyword)), cond, do_t, body, token(closing)),
            )
        } else {
            let body = statements.and_then(|s| self.visit(&s.as_node()))?;
            let cond = self.visit(&predicate)?;
            Some(self.builder.loop_mod(ty, body, token(Some(keyword)), cond))
        }
    }

    fn visit_string_node(&mut self, node: &Node<'_>) -> NodeId {
        let n = node.as_string_node().unwrap();
        let opening = n.opening_loc();
        let opening_slice = opening.as_ref().map(Location::as_slice);
        if let Some(open) = &opening
            && open.as_slice().starts_with(b"<<")
        {
            let part = Part::Str {
                unescaped: n.unescaped(),
                content: n.content_loc().as_slice(),
                location: n.content_loc(),
            };
            return self.visit_heredoc(
                NodeType::Str,
                opening.unwrap(),
                n.closing_loc().unwrap(),
                vec![part],
            );
        }
        if opening_slice == Some(b"?") {
            return self
                .builder
                .character_bytes(n.unescaped(), range(&node.location()));
        }
        if opening_slice.is_some_and(|o| o.starts_with(b"%")) && n.unescaped().is_empty() {
            return self
                .builder
                .string_compose(token(opening), Vec::new(), token(n.closing_loc()));
        }
        let content = n.content_loc();
        let parts = if content.as_slice().contains(&b'\n') {
            self.string_nodes_from_line_continuations(
                n.unescaped(),
                content.as_slice(),
                content.start() as usize,
                opening_slice,
            )
        } else {
            vec![
                self.builder
                    .string_internal_bytes(n.unescaped(), range(&content)),
            ]
        };
        self.builder
            .string_compose(token(opening), parts, token(n.closing_loc()))
    }

    fn visit_symbol_node(&mut self, node: &Node<'_>) -> NodeId {
        let n = node.as_symbol_node().unwrap();
        match n.closing_loc() {
            None => {
                let t = Some(Tok::new(lossy(n.unescaped()), range(&node.location())));
                if n.opening_loc().is_none() {
                    self.builder.symbol_internal(t)
                } else {
                    self.builder.symbol(t)
                }
            }
            Some(closing) => {
                let value = Some(n.content_loc());
                let value_slice = value.as_ref().map_or(&b""[..], Location::as_slice);
                let parts = if value_slice.is_empty() {
                    Vec::new()
                } else if value_slice.contains(&b'\n') {
                    let value = value.unwrap();
                    self.string_nodes_from_line_continuations(
                        n.unescaped(),
                        value_slice,
                        value.start() as usize,
                        n.opening_loc().as_ref().map(Location::as_slice),
                    )
                } else {
                    vec![
                        self.builder
                            .string_internal_bytes(n.unescaped(), range(&value.unwrap())),
                    ]
                };
                self.builder
                    .symbol_compose(token(n.opening_loc()), parts, token(Some(closing)))
            }
        }
    }

    /// The parts of a regexp or xstring literal.
    fn literal_parts(
        &mut self,
        unescaped: &[u8],
        content: Location<'_>,
        opening: Option<Location<'_>>,
        _always: bool,
    ) -> Vec<NodeId> {
        let slice = content.as_slice();
        if slice.is_empty() {
            Vec::new()
        } else if slice.contains(&b'\n') {
            self.string_nodes_from_line_continuations(
                unescaped,
                slice,
                content.start() as usize,
                opening.as_ref().map(Location::as_slice),
            )
        } else {
            vec![
                self.builder
                    .string_internal_bytes(unescaped, range(&content)),
            ]
        }
    }

    fn regexp(
        &mut self,
        opening: Location<'_>,
        parts: Vec<NodeId>,
        closing: Location<'_>,
    ) -> NodeId {
        let closing_slice = closing.as_slice();
        let start = closing.start() as usize;
        let end_t = Some(Tok::new(
            lossy(&closing_slice[..1.min(closing_slice.len())]),
            Range::new(start, start + 1),
        ));
        let options = self.builder.regexp_options(Some(Tok::new(
            lossy(closing_slice.get(1..).unwrap_or_default()),
            Range::new(start + 1, closing.end() as usize),
        )));
        self.builder
            .regexp_compose(token(Some(opening)), parts, end_t, options)
    }

    fn visit_block_parameters(
        &mut self,
        node: &ruby_prism::BlockParametersNode<'_>,
    ) -> Vec<NodeId> {
        let mut params = match node.parameters() {
            Some(p) => self.visit_parameters(&p),
            None => Vec::new(),
        };
        params.extend(self.visit_all(node.locals().iter()));
        params
    }

    fn visit_parameters(&mut self, node: &ruby_prism::ParametersNode<'_>) -> Vec<NodeId> {
        let mut params = Vec::new();
        for required in node.requireds().iter() {
            let visited = if matches!(required, Node::RequiredParameterNode { .. }) {
                self.visit(&required)
            } else {
                self.with_destructure(|c| c.visit(&required))
            };
            params.extend(visited);
        }
        params.extend(self.visit_all(node.optionals().iter()));
        if let Some(rest) = node.rest()
            && !matches!(rest, Node::ImplicitRestNode { .. })
        {
            params.extend(self.visit(&rest));
        }
        for post in node.posts().iter() {
            let visited = if matches!(post, Node::RequiredParameterNode { .. }) {
                self.visit(&post)
            } else {
                self.with_destructure(|c| c.visit(&post))
            };
            params.extend(visited);
        }
        params.extend(self.visit_all(node.keywords().iter()));
        if let Some(keyword_rest) = node.keyword_rest() {
            params.extend(self.visit(&keyword_rest));
        }
        if let Some(block) = node.block() {
            params.extend(self.visit(&block));
        }
        params
    }

    fn visit_block(&mut self, call: NodeId, block: Option<ruby_prism::BlockNode<'_>>) -> NodeId {
        let Some(block) = block else {
            return call;
        };
        let args = match block.parameters() {
            None => self.builder.args(None, Vec::new(), None),
            Some(parameters) => match parameters {
                Node::NumberedParametersNode { .. } | Node::ItParametersNode { .. } => {
                    self.visit(&parameters).unwrap()
                }
                _ => {
                    let p = parameters.as_block_parameters_node().unwrap();
                    let list = self.visit_block_parameters(&p);
                    self.builder
                        .args(token(p.opening_loc()), list, token(p.closing_loc()))
                }
            },
        };
        let body = self.visit_opt(block.body());
        self.builder.block(
            call,
            token(Some(block.opening_loc())),
            args,
            body,
            token(Some(block.closing_loc())),
        )
    }

    fn visit_heredoc(
        &mut self,
        ty: NodeType,
        opening: Location<'_>,
        closing: Location<'_>,
        parts: Vec<Part<'_>>,
    ) -> NodeId {
        let mut children: Vec<NodeId> = Vec::new();
        let mut indented = false;
        let opening_slice = opening.as_slice();

        if opening_slice.starts_with(b"<<~")
            && let Some(Part::Node(first)) = parts.first()
            && !matches!(first, Node::StringNode { .. })
        {
            let location = first.location();
            let start = location.start() as usize;
            let line_start = self.source()[..start]
                .iter()
                .rposition(|&b| b == b'\n')
                .map_or(0, |i| i + 1);
            let value = &self.source()[line_start..location.end() as usize];
            children.push(
                self.builder
                    .string_internal_bytes(value, Range::new(line_start, location.end() as usize)),
            );
            indented = true;
        }

        for part in parts {
            let pushing = match part {
                Part::Str {
                    unescaped,
                    content,
                    location,
                } => {
                    if content.contains(&b'\n') {
                        self.string_nodes_from_line_continuations(
                            unescaped,
                            content,
                            location.start() as usize,
                            Some(opening_slice),
                        )
                    } else {
                        let s = self
                            .builder
                            .string_internal_bytes(unescaped, range(&location));
                        vec![s]
                    }
                }
                Part::Node(node) => {
                    if let Some(s) = node.as_string_node()
                        && s.content_loc().as_slice().contains(&b'\n')
                    {
                        self.string_nodes_from_line_continuations(
                            s.unescaped(),
                            s.content_loc().as_slice(),
                            node.location().start() as usize,
                            Some(opening_slice),
                        )
                    } else {
                        self.visit(&node).into_iter().collect()
                    }
                }
            };

            for child in pushing {
                let child_node = self.builder.node(child);
                let is_str = child_node.ty == NodeType::Str;
                let child_bytes = child_node.children.first().and_then(Child::str_bytes);
                if is_str && child_bytes == Some(&b""[..]) {
                    continue;
                }
                if is_str
                    && let Some(&last) = children.last()
                    && self.builder.ty(last) == NodeType::Str
                    && !self
                        .builder
                        .node(last)
                        .children
                        .first()
                        .and_then(Child::str_bytes)
                        .is_some_and(|s| s.ends_with(b"\n"))
                {
                    let appended = child_bytes.unwrap_or_default().to_vec();
                    let child_expr = self.builder.expr(child);
                    let last_expr = self.builder.expr(last);
                    let mut merged = self.builder.node(last).clone();
                    let mut base = merged
                        .children
                        .first()
                        .and_then(Child::str_bytes)
                        .unwrap_or_default()
                        .to_vec();
                    base.extend_from_slice(&appended);
                    merged.children = vec![Child::string(base)];
                    merged.loc.expression = Some(join(last_expr, child_expr));
                    let id = self.builder.n(merged.ty, merged.children, merged.loc);
                    *children.last_mut().unwrap() = id;
                } else {
                    children.push(child);
                }
            }
        }

        let closing_slice = closing.as_slice();
        let trailing = trailing_space_match(closing_slice);
        let closing_t = Some(Tok::new(
            lossy(chomp(closing_slice)),
            Range::new(closing.start() as usize, closing.end() as usize - trailing),
        ));
        let composed = if ty == NodeType::Xstr {
            self.builder
                .xstring_compose(token(Some(opening)), children.clone(), closing_t)
        } else {
            self.builder
                .string_compose(token(Some(opening)), children.clone(), closing_t)
        };
        if indented {
            self.builder.node_mut(composed).children =
                children[1..].iter().map(|&c| Child::Node(c)).collect();
        }
        composed
    }

    fn string_nodes_from_interpolation<'pr>(
        &mut self,
        parts: impl IntoIterator<Item = Node<'pr>>,
        opening: Option<Location<'_>>,
    ) -> Vec<NodeId> {
        let opening = opening.as_ref().map(Location::as_slice);
        let mut out = Vec::new();
        for part in parts {
            if let Some(s) = part.as_string_node()
                && s.content_loc().as_slice().contains(&b'\n')
                && s.opening_loc().is_none()
            {
                out.extend(self.string_nodes_from_line_continuations(
                    s.unescaped(),
                    s.content_loc().as_slice(),
                    s.content_loc().start() as usize,
                    opening,
                ));
            } else {
                out.extend(self.visit(&part));
            }
        }
        out
    }

    fn string_nodes_from_line_continuations(
        &mut self,
        unescaped: &[u8],
        escaped: &[u8],
        start_offset: usize,
        opening: Option<&[u8]>,
    ) -> Vec<NodeId> {
        let unescaped = lines(unescaped);
        let escaped = lines(escaped);
        let percent_array = opening.is_some_and(|o| {
            o.starts_with(b"%w")
                || o.starts_with(b"%W")
                || o.starts_with(b"%i")
                || o.starts_with(b"%I")
        });
        let regex = opening.is_some_and(|o| o == b"/" || o.starts_with(b"%r"));
        let mut start_offset = start_offset;
        let mut out = Vec::new();

        let non_interpolating = opening.is_some_and(|o| {
            o.ends_with(b"'")
                || o.starts_with(b"%q")
                || o.starts_with(b"%s")
                || o.starts_with(b"%w")
                || o.starts_with(b"%i")
        });
        if non_interpolating {
            let mut current_length = 0;
            let mut current_line: Vec<u8> = Vec::new();
            for (index, escaped_line) in escaped.iter().enumerate() {
                let unescaped_line = unescaped.get(index).copied().unwrap_or_default();
                current_length += escaped_line.len();
                current_line.extend_from_slice(unescaped_line);
                if percent_array && escaped_line.ends_with(b"\\\n") && index != escaped.len() - 1 {
                    continue;
                }
                let s = self.builder.string_internal_bytes(
                    &current_line,
                    Range::new(start_offset, start_offset + current_length),
                );
                start_offset += escaped_line.len();
                current_line.clear();
                current_length = 0;
                out.push(s);
            }
            return out;
        }

        let mut escaped_lengths: Vec<usize> = Vec::new();
        let mut normalized_lengths: Vec<usize> = Vec::new();
        let mut do_next_tokens: Vec<bool> = Vec::new();

        // escaped.chunk_while { |before, _| odd trailing backslashes }
        let mut chunks: Vec<Vec<&[u8]>> = Vec::new();
        for line in &escaped {
            let continues = chunks
                .last()
                .and_then(|chunk| chunk.last())
                .is_some_and(|before| trailing_backslashes(before) % 2 == 1);
            if continues {
                chunks.last_mut().unwrap().push(line);
            } else {
                chunks.push(vec![line]);
            }
        }
        for chunk in &chunks {
            let total: usize = chunk.iter().map(|l| l.len()).sum();
            escaped_lengths.push(total);
            let unescaped_lines_count: usize = if regex {
                0
            } else {
                chunk
                    .iter()
                    .map(|line| {
                        let mut count = escaped_newline_count(line);
                        if !line.ends_with(b"\n") && count > 0 {
                            count -= 1;
                        }
                        count
                    })
                    .sum()
            };
            let extra = if percent_array { chunk.len() } else { 1 };
            let n = unescaped_lines_count + extra;
            normalized_lengths.extend(std::iter::repeat_n(0, n));
            *normalized_lengths.last_mut().unwrap() = total;
            do_next_tokens.extend(std::iter::repeat_n(false, n));
            *do_next_tokens.last_mut().unwrap() = true;
        }

        let mut current_line: Vec<u8> = Vec::new();
        let mut current_normalized_length = 0;
        let mut emitted_count = 0;
        for (index, unescaped_line) in unescaped.iter().enumerate() {
            current_line.extend_from_slice(unescaped_line);
            current_normalized_length += normalized_lengths.get(index).copied().unwrap_or(0);
            if do_next_tokens.get(index).copied().unwrap_or(false) {
                let s = self.builder.string_internal_bytes(
                    &current_line,
                    Range::new(start_offset, start_offset + current_normalized_length),
                );
                start_offset += escaped_lengths.get(emitted_count).copied().unwrap_or(0);
                current_line.clear();
                current_normalized_length = 0;
                emitted_count += 1;
                out.push(s);
            }
        }
        out
    }
}

/// `line.scan(/(\\*)n/).count { |(backslashes)| backslashes.length.odd? }`
fn escaped_newline_count(line: &[u8]) -> usize {
    let mut count = 0;
    let mut run = 0;
    for &b in line {
        if b == b'\\' {
            run += 1;
        } else {
            if b == b'n' && run % 2 == 1 {
                count += 1;
            }
            run = 0;
        }
    }
    count
}

fn chomp_eq(slice: &[u8]) -> &[u8] {
    slice.strip_suffix(b"=").unwrap_or(slice)
}

fn find_forwarding(node: Option<ruby_prism::ParametersNode<'_>>) -> Vec<&'static str> {
    let Some(node) = node else {
        return Vec::new();
    };
    let mut forwarding = Vec::new();
    if let Some(rest) = node.rest()
        && let Some(rest) = rest.as_rest_parameter_node()
        && rest.name().is_none()
    {
        forwarding.push("*");
    }
    match node.keyword_rest() {
        Some(Node::KeywordRestParameterNode { .. }) => {
            if node
                .keyword_rest()
                .unwrap()
                .as_keyword_rest_parameter_node()
                .unwrap()
                .name()
                .is_none()
            {
                forwarding.push("**");
            }
        }
        Some(Node::ForwardingParameterNode { .. }) => {}
        _ => {}
    }
    if let Some(block) = node.block()
        && block
            .as_block_parameter_node()
            .is_some_and(|b| b.name().is_none())
    {
        forwarding.push("&");
    }
    if let Some(Node::ForwardingParameterNode { .. }) = node.keyword_rest() {
        for f in ["&", "..."] {
            if !forwarding.contains(&f) {
                forwarding.push(f);
            }
        }
    }
    forwarding
}
