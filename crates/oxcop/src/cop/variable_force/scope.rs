//! Port of `lib/rubocop/cop/variable_force/scope.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::variable_force::{SCOPE_TYPES, VariableId};

/// `OUTER_SCOPE_CHILD_INDICES`: the children of a scope node that belong to
/// the outer scope (inclusive range of child indices).
fn outer_scope_child_indices(ty: NodeType) -> Option<std::ops::RangeInclusive<usize>> {
    match ty {
        NodeType::Defs | NodeType::Module | NodeType::Sclass | NodeType::Block => Some(0..=0),
        NodeType::Class => Some(0..=1),
        _ => None,
    }
}

/// A context of local variable visibility: a scope node and the variables
/// that belong to it, in declaration order.
#[derive(Debug)]
pub struct Scope<'a> {
    pub node: Node<'a>,
    /// `variables`: name to variable, in insertion order (a Ruby hash).
    variables: Vec<(&'a str, VariableId)>,
    naked_top_level: bool,
}

impl<'a> Scope<'a> {
    pub fn new(node: Node<'a>) -> Self {
        // Any node type is accepted for the top level scope.
        let naked_top_level = !SCOPE_TYPES.contains(&node.ty());
        Self {
            node,
            variables: Vec::new(),
            naked_top_level,
        }
    }

    pub fn naked_top_level(&self) -> bool {
        self.naked_top_level
    }

    /// The ids of `variables.each_value`, in order.
    pub fn variable_ids(&self) -> impl Iterator<Item = VariableId> + '_ {
        self.variables.iter().map(|&(_, id)| id)
    }

    pub fn variable_id(&self, name: &str) -> Option<VariableId> {
        self.variables
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, id)| id)
    }

    /// `variables[name] = variable` (an existing key keeps its position).
    pub(crate) fn set_variable(&mut self, name: &'a str, id: VariableId) {
        match self.variables.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = id,
            None => self.variables.push((name, id)),
        }
    }

    /// `==`: the same scope node.
    pub fn same_scope(&self, other: &Scope<'_>) -> bool {
        self.node.id() == other.node.id()
    }

    pub fn name(&self) -> Option<&'a str> {
        self.node.method_name()
    }

    pub fn body_node(&self) -> Option<Node<'a>> {
        body_node(self.node, self.naked_top_level)
    }

    pub fn include(&self, target: Node<'_>) -> bool {
        include(self.node, target)
    }

    /// `each_node`: the nodes of this scope (not of inner scopes).
    pub fn each_node(&self) -> Vec<Node<'a>> {
        let mut nodes = Vec::new();
        if self.naked_top_level {
            nodes.push(self.node);
        }
        scan_node(self.node, self.node, &mut nodes);
        nodes
    }
}

fn is_naked(scope_node: Node<'_>) -> bool {
    !SCOPE_TYPES.contains(&scope_node.ty())
}

pub(crate) fn body_node(node: Node<'_>, naked_top_level: bool) -> Option<Node<'_>> {
    if naked_top_level {
        return Some(node);
    }
    let index = match node.ty() {
        NodeType::Module | NodeType::Sclass => 1,
        NodeType::Def
        | NodeType::Class
        | NodeType::Block
        | NodeType::Numblock
        | NodeType::Itblock => 2,
        NodeType::Defs => 3,
        _ => return None,
    };
    node.child(index)
}

/// `include?(target_node)` of the scope whose node is `scope_node`.
pub(crate) fn include(scope_node: Node<'_>, target: Node<'_>) -> bool {
    !belong_to_outer_scope(scope_node, target) && !belong_to_inner_scope(scope_node, target)
}

fn scan_node<'a>(scope_node: Node<'a>, node: Node<'a>, out: &mut Vec<Node<'a>>) {
    for child in node.child_nodes() {
        if !include(scope_node, child) {
            continue;
        }
        out.push(child);
        scan_node(scope_node, child, out);
    }
}

fn belong_to_outer_scope(scope_node: Node<'_>, target: Node<'_>) -> bool {
    if !is_naked(scope_node) && target.id() == scope_node.id() {
        return true;
    }
    if ancestor_node(scope_node, target) {
        return true;
    }
    let Some(parent) = target.parent() else {
        return false;
    };
    if parent.id() != scope_node.id() {
        return false;
    }
    let Some(indices) = outer_scope_child_indices(parent.ty()) else {
        return false;
    };
    target.sibling_index().is_some_and(|i| indices.contains(&i))
}

fn belong_to_inner_scope(scope_node: Node<'_>, target: Node<'_>) -> bool {
    let Some(parent) = target.parent() else {
        return false;
    };
    if parent.id() == scope_node.id() {
        return false;
    }
    if !SCOPE_TYPES.contains(&parent.ty()) {
        return false;
    }
    let Some(indices) = outer_scope_child_indices(parent.ty()) else {
        return true;
    };
    !target.sibling_index().is_some_and(|i| indices.contains(&i))
}

fn ancestor_node(scope_node: Node<'_>, target: Node<'_>) -> bool {
    scope_node.ancestors().any(|a| a.id() == target.id())
}
