//! Port of `lib/rubocop/cop/variable_force/assignment.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::variable_force::branch::{self, Branch};
use crate::cop::variable_force::variable::Variable;
use crate::cop::variable_force::{
    OPERATOR_ASSIGNMENT_TYPES, VARIABLE_ASSIGNMENT_TYPES, VariableId, branchable,
};

/// One assignment of a variable.
#[derive(Debug, Clone)]
pub struct Assignment<'a> {
    pub node: Node<'a>,
    pub variable: VariableId,
    pub referenced: bool,
    pub references: Vec<Node<'a>>,
    pub reassigned: bool,
    pub branch: Option<Branch<'a>>,
    meta_assignment_node: Option<Node<'a>>,
}

impl<'a> Assignment<'a> {
    /// `variable_scope` is the scope node of the assigned variable.
    pub fn new(node: Node<'a>, variable: VariableId, variable_scope: Node<'a>) -> Self {
        assert!(
            VARIABLE_ASSIGNMENT_TYPES.contains(&node.ty()),
            "Node type must be any of {VARIABLE_ASSIGNMENT_TYPES:?}, passed {:?}",
            node.ty()
        );
        let meta_assignment_node = operator_assignment_node(node)
            .or_else(|| multiple_assignment_node(node))
            .or_else(|| rest_assignment_node(node))
            .or_else(|| for_assignment_node(node));
        Self {
            node,
            variable,
            referenced: false,
            references: Vec::new(),
            reassigned: false,
            branch: branch::of(node, variable_scope),
            meta_assignment_node,
        }
    }

    pub fn name(&self) -> Option<&'a str> {
        self.node.child_sym(0)
    }

    pub fn reference(&mut self, node: Node<'a>) {
        self.references.push(node);
        self.referenced = true;
    }

    pub fn mark_reassigned(&mut self) {
        if self.referenced {
            return;
        }
        self.reassigned = true;
    }

    /// `used?`; `variable` is the assigned variable.
    pub fn is_used(&self, variable: &Variable<'_>) -> bool {
        (!self.reassigned && variable.captured_by_block) || self.referenced
    }

    pub fn is_regexp_named_capture(&self) -> bool {
        self.node.is(NodeType::MatchWithLvasgn)
    }

    pub fn is_exception_assignment(&self) -> bool {
        self.node.parent().is_some_and(|parent| {
            parent.is(NodeType::Resbody) && parent.exception_variable() == Some(self.node)
        })
    }

    pub fn is_operator_assignment(&self) -> bool {
        self.meta_assignment_node
            .is_some_and(|meta| OPERATOR_ASSIGNMENT_TYPES.contains(&meta.ty()))
    }

    pub fn is_multiple_assignment(&self) -> bool {
        self.meta_assignment_node
            .is_some_and(|meta| meta.is(NodeType::Masgn))
    }

    pub fn is_rest_assignment(&self) -> bool {
        self.meta_assignment_node
            .is_some_and(|meta| meta.is(NodeType::Splat))
    }

    pub fn is_for_assignment(&self) -> bool {
        self.meta_assignment_node
            .is_some_and(|meta| meta.is(NodeType::For))
    }

    pub fn operator(&self) -> &'a str {
        let assignment_node = self.meta_assignment_node.unwrap_or(self.node);
        assignment_node
            .loc()
            .operator
            .map_or("", |op| assignment_node.range_source(op))
    }

    pub fn meta_assignment_node(&self) -> Option<Node<'a>> {
        self.meta_assignment_node
    }

    pub fn run_exclusively_with(&self, other: Option<&Branch<'a>>) -> bool {
        branchable::run_exclusively_with(self.branch.as_ref(), other)
    }
}

fn operator_assignment_node(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    if !OPERATOR_ASSIGNMENT_TYPES.contains(&parent.ty()) {
        return None;
    }
    if node.sibling_index() != Some(0) {
        return None;
    }
    Some(parent)
}

fn multiple_assignment_node(node: Node<'_>) -> Option<Node<'_>> {
    let mut candidate_mlhs_node = node.parent()?;
    // In `(foo, bar), *baz`, the splat node must be traversed as well.
    if candidate_mlhs_node.is(NodeType::Splat) {
        candidate_mlhs_node = candidate_mlhs_node.parent()?;
    }
    if !candidate_mlhs_node.is(NodeType::Mlhs) {
        return None;
    }
    let grandparent_node = node.parent()?.parent()?;
    if let Some(found) = find_multiple_assignment_node(grandparent_node) {
        return Some(found);
    }
    if !grandparent_node.is(NodeType::Masgn) {
        return None;
    }
    Some(grandparent_node)
}

fn rest_assignment_node(node: Node<'_>) -> Option<Node<'_>> {
    let parent = node.parent()?;
    parent.is(NodeType::Splat).then_some(parent)
}

fn for_assignment_node(node: Node<'_>) -> Option<Node<'_>> {
    let parent_node = node.parent()?;
    if parent_node.is(NodeType::For) {
        return Some(parent_node);
    }
    let grandparent_node = parent_node.parent();
    if parent_node.is(NodeType::Mlhs)
        && let Some(grandparent_node) = grandparent_node
        && grandparent_node.is(NodeType::For)
    {
        return Some(grandparent_node);
    }
    None
}

fn find_multiple_assignment_node(grandparent_node: Node<'_>) -> Option<Node<'_>> {
    if !grandparent_node.is(NodeType::Mlhs) {
        return None;
    }
    let parent = grandparent_node.parent()?;
    if parent.is(NodeType::Masgn) {
        return Some(parent);
    }
    find_multiple_assignment_node(parent)
}
