//! Port of `lib/rubocop/cop/metrics/utils/repeated_attribute_discount.rb`:
//! identifies repetitions of `(c)send` calls with no arguments.
//!
//! ```ruby
//! foo.bar
//! foo.bar # => repeated
//! foo.bar.baz.qux # => inner send repeated
//! foo.bar.baz.other # => both inner send repeated
//! foo.bar(2) # => not repeated
//! ```
//!
//! It also invalidates sequences if a receiver is reassigned:
//!
//! ```ruby
//! xx.foo.bar
//! xx.foo.baz      # => inner send repeated
//! self.xx = any   # => invalidates everything so far
//! xx.foo.baz      # => no repetition
//! self.xx.foo.baz # => all repeated
//! ```
//!
//! Upstream keeps nested hashes keyed by root nodes (compared
//! structurally) and method names; here they live in an arena.

use std::collections::HashMap;

use crate::ast::{Child, Node, NodeType};

/// A key of a "known attributes" hash: a root node (as an s-expression)
/// or a method name.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Key {
    Root(String),
    Method(String),
}

/// The receiver of a setter, as `setter_to_getter` returns it.
enum Receiver<'a> {
    Node(Option<Node<'a>>),
    /// `s(type, name)` for a variable assignment.
    Var(String),
    /// Anything else (e.g. the symbol child of `(lvasgn :x)` in `x ||= 1`):
    /// `find_attributes` yields `nil` for it.
    Other,
}

pub struct RepeatedAttributeDiscount {
    hashes: Vec<HashMap<Key, usize>>,
}

const KNOWN: usize = 0;

impl RepeatedAttributeDiscount {
    pub fn new() -> Self {
        // Share hash for `(send nil? :foo)` and `(send (self) :foo)`
        let mut known = HashMap::new();
        known.insert(Key::Root("nil".to_owned()), 1);
        known.insert(Key::Root(sexp_self()), 1);
        Self {
            hashes: vec![known, HashMap::new()],
        }
    }

    /// `discount_repeated_attribute?(send_node)`.
    pub fn discount_repeated_attribute(&mut self, send_node: Node<'_>) -> bool {
        if !attribute_call(send_node) {
            return false;
        }
        let mut repeated = true;
        let found = self.find_attributes(Receiver::Node(Some(send_node)), &mut |this, hash, key| {
            repeated = false;
            let new = this.hashes.len();
            this.hashes.push(HashMap::new());
            this.hashes[hash].insert(key, new);
            Some(new)
        });
        found.is_some() && repeated
    }

    /// `update_repeated_attribute(node)`.
    pub fn update_repeated_attribute(&mut self, node: Node<'_>) {
        let Some((receiver, method)) = setter_to_getter(node) else {
            return;
        };
        let Some(calls) = self.find_attributes(receiver, &mut |_, _, _| None) else {
            return;
        };
        match method {
            // e.g. `self.foo = 42`
            Some(method) => {
                self.hashes[calls].remove(&Key::Method(method));
            }
            // e.g. `var = 42`
            None => self.hashes[calls].clear(),
        }
    }

    /// `find_attributes`: the hash for `node`, walking the receiver chain.
    /// When a step is missing, `missing(self, hash, key)` decides (`None`
    /// stops); when the node is not a chain of argument-less calls on a
    /// root, the result is `None`.
    fn find_attributes(
        &mut self,
        node: Receiver<'_>,
        missing: &mut dyn FnMut(&mut Self, usize, Key) -> Option<usize>,
    ) -> Option<usize> {
        let (calls, value) = match node {
            Receiver::Node(Some(n)) if attribute_call(n) => {
                let calls = self.find_attributes(Receiver::Node(n.receiver()), missing)?;
                (calls, Key::Method(n.method_name().unwrap_or("").to_owned()))
            }
            Receiver::Node(n) if root_node(n) => (KNOWN, Key::Root(n.map_or("nil".to_owned(), sexp))),
            Receiver::Var(key) => (KNOWN, Key::Root(key)),
            _ => return None,
        };
        match self.hashes[calls].get(&value) {
            Some(&found) => Some(found),
            None => missing(self, calls, value),
        }
    }
}

impl Default for RepeatedAttributeDiscount {
    fn default() -> Self {
        Self::new()
    }
}

/// `(call _receiver _method)`: a call without arguments.
fn attribute_call(node: Node<'_>) -> bool {
    node.is_call() && node.children().len() == 2
}

/// `root_node?`: `nil`, `self`, a variable or a constant.
fn root_node(node: Option<Node<'_>>) -> bool {
    match node {
        None => true,
        Some(n) => matches!(
            n.ty(),
            NodeType::SelfNode
                | NodeType::Lvar
                | NodeType::Ivar
                | NodeType::Cvar
                | NodeType::Gvar
                | NodeType::Const
        ),
    }
}

/// `setter_to_getter(node)`: `[receiver, method | nil]` for a setter.
fn setter_to_getter(node: Node<'_>) -> Option<(Receiver<'_>, Option<String>)> {
    let getter = match node.ty() {
        NodeType::Lvasgn => Some("lvar"),
        NodeType::Ivasgn => Some("ivar"),
        NodeType::Cvasgn => Some("cvar"),
        NodeType::Gvasgn => Some("gvar"),
        _ => None,
    };
    if let Some(getter) = getter {
        // (lvasgn :my_var (int 42)) => [(lvar my_var), nil]
        let name = node.name().unwrap_or("");
        let key = format!("({getter} {:?})", Child::Sym(name.to_owned()));
        return Some((Receiver::Var(key), None));
    }
    if node.is_shorthand_asgn() {
        // (or-asgn (send _receiver :foo) _value) => [_receiver, :foo]
        let lhs = node.child(0)?;
        let receiver = match lhs.children().first() {
            Some(Child::Node(id)) => Receiver::Node(Some(node.ast().node(*id))),
            Some(Child::Nil) => Receiver::Node(None),
            _ => Receiver::Other,
        };
        let method = match lhs.children().get(1) {
            Some(Child::Sym(name)) => Some(name.clone()),
            None | Some(Child::Nil) => None,
            // A node (`a[0] ||= 1`): no method name equals it.
            Some(_) => Some("\0".to_owned()),
        };
        return Some((receiver, method));
    }
    if node.is_call() && node.loc().operator.is_some() {
        // (send _receiver :foo= (int 42) ) => [_receiver, :foo]
        let name = node.method_name().unwrap_or("");
        let method = name.strip_suffix('=').unwrap_or(name).to_owned();
        return Some((Receiver::Node(node.receiver()), Some(method)));
    }
    if node.is(NodeType::Indexasgn) && node.loc().operator.is_some() {
        // `a[0] = 1`: `:[]=` without the `=`.
        return Some((Receiver::Node(node.child(0)), Some("[]".to_owned())));
    }
    None
}

fn sexp_self() -> String {
    "(self)".to_owned()
}

/// A structural key for a node (Ruby compares nodes with `==`).
fn sexp(node: Node<'_>) -> String {
    let mut out = format!("({}", node.ty().name());
    for child in node.children() {
        out.push(' ');
        match child {
            Child::Node(id) => out.push_str(&sexp(node.ast().node(*id))),
            other => out.push_str(&format!("{other:?}")),
        }
    }
    out.push(')');
    out
}
