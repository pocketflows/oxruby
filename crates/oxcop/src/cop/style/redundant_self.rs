//! Port of `lib/rubocop/cop/style/redundant_self.rb`.
//!
//! Ruby keeps `@local_variables_scopes`, an identity hash from nodes to
//! arrays, where `add_scope` makes many nodes share one array. Here the
//! arrays live in `lists` and nodes map to an index into it. Both maps are
//! keyed by node identity, so they are reset for every investigation.

use std::collections::{HashMap, HashSet};

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Redundant `self` detected.";

/// `Kernel.methods(false)` under RuboCop (Ruby 3.3, with `pathname` loaded
/// and Bundler's `no_warning_require`).
const KERNEL_METHODS: &[&str] = &[
    "Array",
    "Complex",
    "Float",
    "Hash",
    "Integer",
    "Pathname",
    "Rational",
    "String",
    "__callee__",
    "__dir__",
    "__method__",
    "`",
    "abort",
    "at_exit",
    "autoload",
    "autoload?",
    "binding",
    "block_given?",
    "caller",
    "caller_locations",
    "catch",
    "eval",
    "exec",
    "exit",
    "exit!",
    "fail",
    "fork",
    "format",
    "gets",
    "global_variables",
    "iterator?",
    "lambda",
    "load",
    "local_variables",
    "loop",
    "no_warning_require",
    "open",
    "p",
    "print",
    "printf",
    "proc",
    "putc",
    "puts",
    "raise",
    "rand",
    "readline",
    "readlines",
    "require",
    "require_relative",
    "select",
    "set_trace_func",
    "sleep",
    "spawn",
    "sprintf",
    "srand",
    "syscall",
    "system",
    "test",
    "throw",
    "trace_var",
    "trap",
    "untrace_var",
    "warn",
];

const KEYWORDS: &[&str] = &[
    "alias",
    "and",
    "begin",
    "break",
    "case",
    "class",
    "def",
    "defined?",
    "do",
    "else",
    "elsif",
    "end",
    "ensure",
    "false",
    "for",
    "if",
    "in",
    "module",
    "next",
    "nil",
    "not",
    "or",
    "redo",
    "rescue",
    "retry",
    "return",
    "self",
    "super",
    "then",
    "true",
    "undef",
    "unless",
    "until",
    "when",
    "while",
    "yield",
    "__FILE__",
    "__LINE__",
    "__ENCODING__",
];

#[derive(Default)]
pub struct RedundantSelf {
    allowed_send_nodes: HashSet<NodeId>,
    scopes: HashMap<NodeId, usize>,
    lists: Vec<Vec<String>>,
}

impl Cop for RedundantSelf {
    fn name(&self) -> &'static str {
        "Style/RedundantSelf"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/ColonMethodCall", "Layout/DotPosition"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::OrAsgn,
            NodeType::AndAsgn,
            NodeType::OpAsgn,
            NodeType::Def,
            NodeType::Defs,
            NodeType::Args,
            NodeType::Blockarg,
            NodeType::Masgn,
            NodeType::Lvasgn,
            NodeType::Resbody,
            NodeType::InPattern,
            NodeType::Send,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::If,
            NodeType::While,
            NodeType::Until,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.allowed_send_nodes.clear();
        self.scopes.clear();
        self.lists.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType::*;
        match node.ty() {
            OrAsgn | AndAsgn => {
                let lhs = node.child(0).unwrap();
                self.allow_self(lhs);
                let lhs_name = if lhs.is(Lvasgn) { lhs.name() } else { None };
                self.add_lhs_to_local_variables_scopes(node.child(1), lhs_name);
            }
            OpAsgn => self.allow_self(node.child(0).unwrap()),
            Def | Defs => {
                let list = self.new_list();
                self.add_scope(node, list);
            }
            Args => self.on_args(node),
            Blockarg => self.on_argument(node),
            Masgn => self.add_masgn_lhs_variables(node.child(1), node.child(0).unwrap()),
            Lvasgn => self.add_lhs_to_local_variables_scopes(node.child(1), node.name()),
            Resbody => {
                if let Some(var) = node.exception_variable().filter(|v| v.is(Lvasgn))
                    && let Some(name) = var.name()
                {
                    self.push(node, name);
                }
            }
            InPattern => {
                let names: Vec<&str> = node
                    .descendants()
                    .filter(|d| d.is(MatchVar))
                    .filter_map(|d| d.child_sym(0))
                    .collect();
                for name in names {
                    self.push(node, name);
                }
            }
            Send => self.on_send(cx, node),
            Block | Numblock | Itblock => {
                let list = self.list_of(node);
                self.add_scope(node, list);
            }
            If | While | Until => {
                let descendants: Vec<Node<'a>> = node
                    .descendants()
                    .filter(|d| d.is(Lvasgn) || d.is(Masgn))
                    .collect();
                for descendant in descendants {
                    if descendant.is(Lvasgn) {
                        self.add_lhs_to_local_variables_scopes(node.condition(), descendant.name());
                    } else {
                        self.add_masgn_lhs_variables(node.condition(), descendant.child(0).unwrap());
                    }
                }
            }
            _ => {}
        }
    }
}

impl RedundantSelf {
    fn new_list(&mut self) -> usize {
        self.lists.push(Vec::new());
        self.lists.len() - 1
    }

    /// `@local_variables_scopes[node]` (creating the entry when missing).
    fn list_of(&mut self, node: Node<'_>) -> usize {
        if let Some(&list) = self.scopes.get(&node.id()) {
            return list;
        }
        let list = self.new_list();
        self.scopes.insert(node.id(), list);
        list
    }

    fn push(&mut self, node: Node<'_>, name: &str) {
        let list = self.list_of(node);
        self.lists[list].push(name.to_owned());
    }

    fn includes(&self, node: Node<'_>, name: &str) -> bool {
        self.scopes
            .get(&node.id())
            .is_some_and(|&list| self.lists[list].iter().any(|n| n == name))
    }

    fn add_scope(&mut self, node: Node<'_>, list: usize) {
        for child in node.descendants() {
            self.scopes.insert(child.id(), list);
        }
    }

    fn on_send<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_self_receiver() && regular_method_call(node)) {
            return;
        }
        if node.parent().is_some_and(|p| p.is(NodeType::Mlhs)) {
            return;
        }
        if self.allowed_send_node(node) || it_method_in_block(node) {
            return;
        }
        let receiver = node.receiver().unwrap();
        cx.add_offense_with_correction(receiver.source_range(), MSG, |corrector| {
            corrector.remove(receiver.source_range());
            corrector.remove(node.loc().dot.unwrap());
        });
    }

    fn allowed_send_node(&self, node: Node<'_>) -> bool {
        let name = node.method_name().unwrap_or("");
        self.allowed_send_nodes.contains(&node.id())
            || self.includes(node, name)
            || node.ancestors().any(|a| self.includes(a, name))
            || KERNEL_METHODS.contains(&name)
    }

    fn on_args(&mut self, node: Node<'_>) {
        for arg in node.child_nodes() {
            self.on_argument(arg);
        }
    }

    fn on_argument(&mut self, node: Node<'_>) {
        if node.is(NodeType::Mlhs) {
            self.on_args(node);
        } else if node.is_argument_type() || node.is(NodeType::Procarg0) {
            if let Some(name) = node.name() {
                self.push(node, name);
            } else {
                // `@local_variables_scopes[node] << nil` still creates the entry.
                self.list_of(node);
            }
        }
    }

    fn allow_self(&mut self, node: Node<'_>) {
        if node.is_send() && node.is_self_receiver() {
            self.allowed_send_nodes.insert(node.id());
        }
    }

    fn add_lhs_to_local_variables_scopes(&mut self, rhs: Option<Node<'_>>, lhs: Option<&str>) {
        let Some(rhs) = rhs else {
            return;
        };
        if rhs.is_send() && !rhs.arguments().is_empty() {
            for argument in rhs.arguments() {
                match lhs {
                    Some(name) => self.push(argument, name),
                    None => {
                        self.list_of(argument);
                    }
                }
            }
        } else {
            match lhs {
                Some(name) => self.push(rhs, name),
                None => {
                    self.list_of(rhs);
                }
            }
        }
    }

    fn add_masgn_lhs_variables(&mut self, rhs: Option<Node<'_>>, lhs: Node<'_>) {
        for child in lhs.child_nodes() {
            self.add_lhs_to_local_variables_scopes(rhs, child.child_sym(0));
        }
    }
}

/// Respects `Lint/ItWithoutArgumentsInBlock` and Ruby 3.3's warning.
fn it_method_in_block(node: Node<'_>) -> bool {
    if !node.is_method("it") {
        return false;
    }
    let Some(block_node) = node.ancestors().find(|a| a.is(NodeType::Block)) else {
        return false;
    };
    if block_node.child(1).is_none_or(|args| args.has_source()) {
        return false;
    }
    node.arguments().is_empty() && !node.is_block_literal()
}

fn regular_method_call(node: Node<'_>) -> bool {
    !(node.is_operator_method()
        || node
            .method_name()
            .is_some_and(|m| KEYWORDS.contains(&m))
        || node.is_camel_case_method()
        || node.is_setter_method()
        || node.is_implicit_call())
}
