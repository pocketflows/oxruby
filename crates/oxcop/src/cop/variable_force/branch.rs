//! Port of `lib/rubocop/cop/variable_force/branch.rb`: a conditional
//! branch in a scope (`Branch::If`, `Branch::Rescue`, ...).

use crate::ast::{Child, Node, NodeType};
use crate::cop::variable_force::scope;

/// The branch classes (`CLASSES_BY_TYPE`), named after the type of the
/// control node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    If,
    While,
    Until,
    WhilePost,
    UntilPost,
    Case,
    CaseMatch,
    For,
    And,
    AndAsgn,
    Or,
    OrAsgn,
    OpAsgn,
    Rescue,
    Ensure,
}

impl Kind {
    fn of(ty: NodeType) -> Option<Kind> {
        Some(match ty {
            NodeType::If => Kind::If,
            NodeType::While => Kind::While,
            NodeType::Until => Kind::Until,
            NodeType::WhilePost => Kind::WhilePost,
            NodeType::UntilPost => Kind::UntilPost,
            NodeType::Case => Kind::Case,
            NodeType::CaseMatch => Kind::CaseMatch,
            NodeType::For => Kind::For,
            NodeType::And => Kind::And,
            NodeType::AndAsgn => Kind::AndAsgn,
            NodeType::Or => Kind::Or,
            NodeType::OrAsgn => Kind::OrAsgn,
            NodeType::OpAsgn => Kind::OpAsgn,
            NodeType::Rescue => Kind::Rescue,
            NodeType::Ensure => Kind::Ensure,
            _ => return None,
        })
    }
}

/// `Branch::Base`: a child node of a control node, in a scope (given by
/// its node).
#[derive(Debug, Clone, Copy)]
pub struct Branch<'a> {
    pub kind: Kind,
    pub child_node: Node<'a>,
    pub scope: Node<'a>,
}

impl PartialEq for Branch<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.control_node() == other.control_node() && self.child_node == other.child_node
    }
}

impl Eq for Branch<'_> {}

impl std::hash::Hash for Branch<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.control_node().id().hash(state);
    }
}

/// `Branch.of(target_node, scope:)`.
pub fn of<'a>(target_node: Node<'a>, scope: Node<'a>) -> Option<Branch<'a>> {
    for node in std::iter::once(target_node).chain(target_node.ancestors()) {
        let parent = node.parent()?;
        if !scope::include(scope, node) {
            return None;
        }
        let Some(kind) = Kind::of(parent.ty()) else {
            continue;
        };
        let branch = Branch {
            kind,
            child_node: node,
            scope,
        };
        if branch.is_branched() {
            return Some(branch);
        }
    }
    None
}

/// Selects children like `control_node.children[child_index]`.
enum Index {
    At(isize),
    /// `1..-2`
    Middle,
}

impl<'a> Branch<'a> {
    pub fn control_node(&self) -> Node<'a> {
        self.child_node
            .parent()
            .expect("a branch child has a parent")
    }

    /// `define_predicate`: whether `child_node` is the child (or one of the
    /// children) at `index`.
    fn predicate(&self, index: Index) -> bool {
        let children = self.control_node().children();
        let is_child = |c: &Child| c.as_node() == Some(self.child_node.id());
        let len = children.len() as isize;
        match index {
            Index::At(i) => {
                let i = if i < 0 { len + i } else { i };
                i >= 0 && i < len && is_child(&children[i as usize])
            }
            Index::Middle => len >= 3 && children[1..(len - 1) as usize].iter().any(is_child),
        }
    }

    pub fn parent(&self) -> Option<Branch<'a>> {
        of(self.control_node(), self.scope)
    }

    /// `each_ancestor(include_self:)`.
    pub fn ancestors(&self, include_self: bool) -> Vec<Branch<'a>> {
        let mut out = Vec::new();
        if include_self {
            out.push(*self);
        }
        let mut branch = *self;
        while let Some(parent) = branch.parent() {
            out.push(parent);
            branch = parent;
        }
        out
    }

    pub fn is_branched(&self) -> bool {
        !self.is_always_run()
    }

    pub fn is_always_run(&self) -> bool {
        match self.kind {
            // SimpleConditional: conditional_clause?
            Kind::If | Kind::While | Kind::Until | Kind::WhilePost | Kind::UntilPost => {
                self.predicate(Index::At(0))
            }
            // target?
            Kind::Case | Kind::CaseMatch => self.predicate(Index::At(0)),
            // element? || collection?
            Kind::For => self.predicate(Index::At(0)) || self.predicate(Index::At(1)),
            // Operator: left_body?
            Kind::And | Kind::AndAsgn | Kind::Or | Kind::OrAsgn | Kind::OpAsgn => {
                self.predicate(Index::At(0))
            }
            Kind::Rescue => false,
            // ensure_body?
            Kind::Ensure => self.predicate(Index::At(-1)),
        }
    }

    /// `ExceptionHandler#main_body?`
    fn main_body(&self) -> bool {
        matches!(self.kind, Kind::Rescue | Kind::Ensure) && self.predicate(Index::At(0))
    }

    pub fn may_jump_to_other_branch(&self) -> bool {
        self.main_body()
    }

    pub fn may_run_incompletely(&self) -> bool {
        self.main_body()
    }

    /// `rescue_clause?` of `Branch::Rescue`, `when_clause?`/`in_pattern?`
    /// of `Case`/`CaseMatch`.
    pub fn is_middle_clause(&self) -> bool {
        self.predicate(Index::Middle)
    }

    pub fn exclusive_with(&self, other: Option<&Branch<'a>>) -> bool {
        let Some(other) = other else {
            return false;
        };
        if self.may_jump_to_other_branch() {
            return false;
        }
        for other_ancestor in other.ancestors(true) {
            if self.control_node() == other_ancestor.control_node() {
                return self.child_node != other_ancestor.child_node;
            }
        }
        match self.parent() {
            Some(parent) => parent.exclusive_with(Some(other)),
            None => false,
        }
    }
}
