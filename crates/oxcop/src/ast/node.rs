//! Port of `RuboCop::AST::Node` and its subclasses' helper methods. One
//! handle type serves every node type; type-specific helpers return `None`
//! (or a neutral value) when called on a node of another type.

use crate::ast::{Ast, Child, Loc, NodeData, NodeId, NodeType};
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
        write!(f, "Node({}#{})", self.ty().name(), self.id)
    }
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

    pub fn ty(&self) -> NodeType {
        self.data().ty
    }

    pub fn is(&self, ty: NodeType) -> bool {
        self.ty() == ty
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
            .map(|id| Node::new(self.ast, id))
    }

    /// The child nodes (`each_child_node`), skipping `nil` and literals.
    pub fn child_nodes(&self) -> impl Iterator<Item = Node<'a>> + 'a {
        let ast = self.ast;
        self.children()
            .iter()
            .filter_map(Child::as_node)
            .map(move |id| Node::new(ast, id))
    }

    pub fn parent(&self) -> Option<Node<'a>> {
        self.data().parent.map(|id| Node::new(self.ast, id))
    }

    /// Descendants in pre-order (`each_descendant`).
    pub fn descendants(&self) -> impl Iterator<Item = Node<'a>> + 'a {
        let ast = self.ast;
        (self.id + 1..self.data().subtree_end).map(move |id| Node::new(ast, id))
    }

    /// Ancestors from the parent outwards (`each_ancestor`).
    pub fn ancestors(&self) -> impl Iterator<Item = Node<'a>> + 'a {
        std::iter::successors(self.parent(), |node| node.parent())
    }

    /// `node.source_range` / `loc.expression`.
    pub fn source_range(&self) -> Range {
        self.loc().expression.unwrap_or_default()
    }
}
