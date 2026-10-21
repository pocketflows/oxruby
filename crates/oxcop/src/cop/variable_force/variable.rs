//! Port of `lib/rubocop/cop/variable_force/variable.rb`.

use std::collections::HashSet;

use crate::ast::{Node, NodeType};
use crate::cop::variable_force::assignment::Assignment;
use crate::cop::variable_force::branch::Branch;
use crate::cop::variable_force::reference::Reference;
use crate::cop::variable_force::{
    ARGUMENT_DECLARATION_TYPES, ScopeId, VARIABLE_ASSIGNMENT_TYPES, VariableId,
};

/// The existence of a local variable: its declaration node and state.
#[derive(Debug, Clone)]
pub struct Variable<'a> {
    pub id: VariableId,
    pub name: &'a str,
    pub declaration_node: Node<'a>,
    /// The scope the variable belongs to (`VariableTable#scope`).
    pub scope: ScopeId,
    /// The node of that scope.
    pub scope_node: Node<'a>,
    pub assignments: Vec<Assignment<'a>>,
    pub references: Vec<Reference<'a>>,
    pub captured_by_block: bool,
}

impl<'a> Variable<'a> {
    pub fn new(
        id: VariableId,
        name: &'a str,
        declaration_node: Node<'a>,
        scope: ScopeId,
        scope_node: Node<'a>,
    ) -> Self {
        let ty = declaration_node.ty();
        assert!(
            VARIABLE_ASSIGNMENT_TYPES.contains(&ty) || ARGUMENT_DECLARATION_TYPES.contains(&ty),
            "Node type must be a variable declaration type, passed {ty:?}"
        );
        Self {
            id,
            name,
            declaration_node,
            scope,
            scope_node,
            assignments: Vec::new(),
            references: Vec::new(),
            captured_by_block: false,
        }
    }

    pub fn assign(&mut self, node: Node<'a>) {
        let assignment = Assignment::new(node, self.id, self.scope_node);
        self.mark_last_as_reassigned(&assignment);
        self.assignments.push(assignment);
    }

    fn mark_last_as_reassigned(&mut self, assignment: &Assignment<'a>) {
        if self.captured_by_block {
            return;
        }
        let last_branch = self.assignments.last().and_then(|a| a.branch);
        if assignment.branch != last_branch {
            return;
        }
        if let Some(last) = self.assignments.last_mut() {
            last.mark_reassigned();
        }
    }

    pub fn is_referenced(&self) -> bool {
        !self.references.is_empty()
    }

    /// `reference!(node)`
    pub fn reference(&mut self, node: Node<'a>) {
        let reference = Reference::new(node, self.scope_node);
        let reference_branch = reference.branch;
        self.references.push(reference);
        let mut consumed_branches: HashSet<Branch<'a>> = HashSet::new();

        for assignment in self.assignments.iter_mut().rev() {
            if let Some(branch) = &assignment.branch
                && consumed_branches.contains(branch)
            {
                continue;
            }

            if !assignment.run_exclusively_with(reference_branch.as_ref()) {
                assignment.reference(node);
            }

            // Modifier if/unless conditions are special. Assignments made in
            // them do not put the assigned variable in scope to the left of
            // the if/unless keyword. A preceding assignment is needed to put
            // the variable in scope. For this reason we skip to the next
            // assignment here.
            if in_modifier_conditional(assignment.node, node) {
                continue;
            }

            let Some(branch) = assignment.branch else {
                break;
            };
            if Some(branch) == reference_branch {
                break;
            }

            if !branch.may_run_incompletely() {
                consumed_branches.insert(branch);
            }
        }
    }

    pub fn capture_with_block(&mut self) {
        self.captured_by_block = true;
    }

    /// Whether the variable is used in its entire lifetime. Once captured
    /// by a block we can't track it, so it counts as used.
    pub fn is_used(&self) -> bool {
        self.captured_by_block || self.is_referenced()
    }

    pub fn should_be_unused(&self) -> bool {
        self.name.starts_with('_')
    }

    pub fn is_argument(&self) -> bool {
        ARGUMENT_DECLARATION_TYPES.contains(&self.declaration_node.ty())
    }

    pub fn is_method_argument(&self) -> bool {
        self.is_argument() && self.scope_node.is_any_def()
    }

    pub fn is_block_argument(&self) -> bool {
        self.is_argument() && self.scope_node.is(NodeType::Block)
    }

    pub fn is_keyword_argument(&self) -> bool {
        matches!(
            self.declaration_node.ty(),
            NodeType::Kwarg | NodeType::Kwoptarg
        )
    }

    pub fn is_explicit_block_local_variable(&self) -> bool {
        self.declaration_node.is(NodeType::Shadowarg)
    }
}

fn in_modifier_conditional(assignment_node: Node<'_>, reference_node: Node<'_>) -> bool {
    let Some(conditional) = modifier_conditional_of(assignment_node) else {
        return false;
    };
    // The out-of-scope problem only affects a reference in the modifier body
    // (to the left of the keyword); a reference after the modifier is put in
    // scope by the condition's assignment, so an earlier assignment there is
    // genuinely useless.
    covers(Some(conditional), reference_node) && !covers(conditional.condition(), reference_node)
}

fn modifier_conditional_of(node: Node<'_>) -> Option<Node<'_>> {
    node.ancestors().find(|conditional| {
        matches!(
            conditional.ty(),
            NodeType::If | NodeType::While | NodeType::Until
        ) && conditional.is_modifier_form()
            && covers(conditional.condition(), node)
    })
}

fn covers(container: Option<Node<'_>>, node: Node<'_>) -> bool {
    let Some(container) = container else {
        return false;
    };
    container.id() == node.id() || container.source_range().contains(node.source_range())
}
