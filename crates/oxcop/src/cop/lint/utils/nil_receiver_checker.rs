//! Port of `lib/rubocop/cop/lint/utils/nil_receiver_checker.rb`. Included
//! by `redundant_safe_navigation.rs`, its only user.

use std::collections::HashSet;

use crate::ast::{Node, NodeId, NodeType};

/// `nil.methods` in a Ruby 3.3 process that has loaded RuboCop.
pub const NIL_CLASS_METHODS: &[&str] = &[
    "!", "!=", "!~", "&", "<=>", "==", "===", "=~", "^", "__id__", "__send__", "class", "clone",
    "define_singleton_method", "display", "dup", "enum_for", "eql?", "equal?", "extend", "freeze",
    "frozen?", "hash", "inspect", "instance_eval", "instance_exec", "instance_of?",
    "instance_variable_defined?", "instance_variable_get", "instance_variable_set",
    "instance_variables", "is_a?", "itself", "kind_of?", "method", "methods", "nil?",
    "object_id", "private_methods", "protected_methods", "public_method", "public_methods",
    "public_send", "rationalize", "remove_instance_variable", "respond_to?", "send",
    "singleton_class", "singleton_method", "singleton_methods", "tap", "then", "to_a", "to_c",
    "to_enum", "to_f", "to_h", "to_i", "to_json", "to_r", "to_s", "to_yaml", "yield_self", "|",
];

pub struct NilReceiverChecker<'a, 'm> {
    receiver: Node<'a>,
    additional_nil_methods: &'m [&'m str],
    checked_nodes: HashSet<NodeId>,
    receiver_binding_name: Option<&'a str>,
    receiver_binding_scope: Option<Node<'a>>,
}

impl<'a, 'm> NilReceiverChecker<'a, 'm> {
    pub fn new(receiver: Node<'a>, additional_nil_methods: &'m [&'m str]) -> Self {
        let mut checker = Self {
            receiver,
            additional_nil_methods,
            checked_nodes: HashSet::new(),
            receiver_binding_name: binding_name(receiver),
            receiver_binding_scope: None,
        };
        if checker.receiver_binding_name.is_some() {
            checker.receiver_binding_scope = checker.binding_scope(receiver);
        }
        checker
    }

    pub fn cant_be_nil(&mut self) -> bool {
        let receiver = self.receiver;
        self.sole_condition_of_parent_if(receiver)
            || self.cant_be_nil_at(receiver.parent(), receiver)
    }

    fn cant_be_nil_at(&mut self, node: Option<Node<'a>>, receiver: Node<'a>) -> bool {
        let Some(node) = node else {
            return false;
        };
        if !self.checked_nodes.insert(node.id()) {
            return false;
        }

        match node.ty() {
            NodeType::Def | NodeType::Defs | NodeType::Class | NodeType::Module | NodeType::Sclass => {
                return false;
            }
            NodeType::Send => {
                if let Some(node_receiver) = node.receiver()
                    && node_receiver.same_as(receiver)
                    && self.same_binding_as_receiver(node_receiver)
                {
                    return self.non_nil_method(node.method_name().unwrap_or_default());
                }
                for argument in node.arguments() {
                    if self.cant_be_nil_at(Some(argument), receiver) {
                        return true;
                    }
                }
                if self.cant_be_nil_at(node.receiver(), receiver) {
                    return true;
                }
            }
            NodeType::Begin => {
                if self.cant_be_nil_at(node.child(0), receiver) {
                    return true;
                }
            }
            NodeType::If | NodeType::Case => {
                if self.cant_be_nil_at(node.condition(), receiver) {
                    return true;
                }
            }
            NodeType::And | NodeType::Or => {
                if self.cant_be_nil_at(node.lhs(), receiver) {
                    return true;
                }
            }
            NodeType::Pair => {
                if self.cant_be_nil_at(node.key(), receiver)
                    || self.cant_be_nil_at(node.pair_value(), receiver)
                {
                    return true;
                }
            }
            NodeType::When => {
                for condition in node.conditions() {
                    if self.cant_be_nil_at(Some(condition), receiver) {
                        return true;
                    }
                }
            }
            NodeType::Lvasgn
            | NodeType::Ivasgn
            | NodeType::Cvasgn
            | NodeType::Gvasgn
            | NodeType::Casgn => {
                if self.cant_be_nil_at(node.expression(), receiver) {
                    return true;
                }
            }
            _ => {}
        }

        if sequentially_reached(node) {
            for sibling in node.left_siblings().into_iter().rev() {
                if self.cant_be_nil_at(Some(sibling), receiver) {
                    return true;
                }
            }
        }

        match node.parent() {
            Some(parent) => self.cant_be_nil_at(Some(parent), receiver),
            None => false,
        }
    }

    fn non_nil_method(&self, method_name: &str) -> bool {
        !NIL_CLASS_METHODS.contains(&method_name) && !self.additional_nil_methods.contains(&method_name)
    }

    fn sole_condition_of_parent_if(&self, node: Node<'a>) -> bool {
        let mut child = Some(node);
        let mut parent = node.parent();
        while let Some(mut p) = parent {
            if p.is(NodeType::If) {
                if !p.is_unless()
                    && let Some(condition) = p.condition()
                    && child != Some(condition)
                    && self.non_nil_condition(condition, node)
                {
                    return true;
                }
                if p.is_elsif() {
                    p = find_top_if(p);
                }
            } else if else_branch(p) {
                match p.parent() {
                    Some(pp) => p = pp,
                    None => return false,
                }
            }
            child = Some(p);
            parent = p.parent();
        }
        false
    }

    fn non_nil_condition(&self, condition: Node<'a>, node: Node<'a>) -> bool {
        if condition.same_as(node) && self.same_binding_as_receiver(condition) {
            return true;
        }
        if !condition.is(NodeType::Csend) {
            return false;
        }

        match csend_root_receiver(condition) {
            Some(root_receiver) => {
                root_receiver.same_as(node) && self.same_binding_as_receiver(root_receiver)
            }
            None => false,
        }
    }

    fn same_binding_as_receiver(&self, occurrence: Node<'a>) -> bool {
        if self.receiver_binding_name.is_none() {
            return true;
        }
        self.binding_scope(occurrence) == self.receiver_binding_scope
    }

    fn binding_scope(&self, occurrence: Node<'a>) -> Option<Node<'a>> {
        let mut node = occurrence;
        while let Some(parent) = node.parent() {
            match parent.ty() {
                NodeType::Block | NodeType::Numblock | NodeType::Itblock => {
                    if parent.body() == Some(node) && self.block_binds_name(parent) {
                        return Some(parent);
                    }
                }
                NodeType::Def
                | NodeType::Defs
                | NodeType::Class
                | NodeType::Module
                | NodeType::Sclass => return Some(parent),
                _ => {}
            }
            node = parent;
        }
        None
    }

    fn block_binds_name(&self, block_node: Node<'a>) -> bool {
        let Some(name) = self.receiver_binding_name else {
            return false;
        };
        if block_node.block_argument_names().iter().any(|a| a == name) {
            return true;
        }
        name == "it" && block_node.arguments().is_empty()
    }
}

fn binding_name(node: Node<'_>) -> Option<&str> {
    if node.is(NodeType::Lvar) {
        node.child_sym(0)
    } else if node.is(NodeType::Send)
        && node.receiver().is_none()
        && node.is_method("it")
        && node.arguments().is_empty()
    {
        Some("it")
    } else {
        None
    }
}

fn sequentially_reached(node: Node<'_>) -> bool {
    if node.is(NodeType::Resbody) {
        return false;
    }
    if let Some(parent) = node.parent()
        && parent.is(NodeType::Ensure)
        && parent.ensure_branch() == Some(node)
    {
        return false;
    }
    !else_branch(node) || (node.is(NodeType::If) && !node.is_elsif())
}

fn else_branch(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        parent.is(NodeType::If) && parent.else_branch().is_some_and(|e| e.same_as(node))
    })
}

fn find_top_if(node: Node<'_>) -> Node<'_> {
    let mut node = node;
    while node.is_elsif() {
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }
    node
}

fn csend_root_receiver(node: Node<'_>) -> Option<Node<'_>> {
    let mut receiver = node.receiver()?;
    while receiver.is_call() {
        match receiver.receiver() {
            Some(inner) => receiver = inner,
            None => break,
        }
    }
    Some(receiver)
}
