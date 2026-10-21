//! Port of `lib/rubocop/cop/variable_force/reference.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::variable_force::branch::{self, Branch};
use crate::cop::variable_force::branchable;

/// A reference of a variable.
#[derive(Debug, Clone)]
pub struct Reference<'a> {
    pub node: Node<'a>,
    /// The scope node of the referenced variable.
    pub scope: Node<'a>,
    pub branch: Option<Branch<'a>>,
}

impl<'a> Reference<'a> {
    pub fn new(node: Node<'a>, scope: Node<'a>) -> Self {
        Self {
            node,
            scope,
            branch: branch::of(node, scope),
        }
    }

    /// Whether the reference is explicit: `super` without arguments and
    /// `binding` reference variables implicitly.
    pub fn is_explicit(&self) -> bool {
        !matches!(self.node.ty(), NodeType::Zsuper | NodeType::Send)
    }

    pub fn run_exclusively_with(&self, other: Option<&Branch<'a>>) -> bool {
        branchable::run_exclusively_with(self.branch.as_ref(), other)
    }
}
