//! Port of `lib/rubocop/cop/variable_force/variable_table.rb`.
//!
//! Scopes and variables live in arenas owned by the table, addressed by
//! [`ScopeId`] and [`VariableId`], so a popped scope (and its variables)
//! stays available to the `after_leaving_scope` hooks.

use crate::ast::Node;
use crate::cop::variable_force::scope::Scope;
use crate::cop::variable_force::variable::Variable;
use crate::cop::variable_force::{ScopeId, VariableId};

/// The hooks a VariableTable invokes on its receiver (the force).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    BeforeEnteringScope,
    AfterEnteringScope,
    BeforeLeavingScope,
    AfterLeavingScope,
    BeforeDeclaringVariable,
    AfterDeclaringVariable,
}

/// `@hook_receiver`.
pub trait HookReceiver<'a> {
    fn scope_hook(&mut self, hook: Hook, scope: ScopeId, table: &VariableTable<'a>);
    fn variable_hook(&mut self, hook: Hook, variable: VariableId, table: &VariableTable<'a>);
}

/// Manages the lifetime of all scopes and local variables in a program.
#[derive(Debug, Default)]
pub struct VariableTable<'a> {
    scopes: Vec<Scope<'a>>,
    scope_stack: Vec<ScopeId>,
    variables: Vec<Variable<'a>>,
}

impl<'a> VariableTable<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scope(&self, id: ScopeId) -> &Scope<'a> {
        &self.scopes[id]
    }

    pub fn variable(&self, id: VariableId) -> &Variable<'a> {
        &self.variables[id]
    }

    pub(crate) fn variable_mut(&mut self, id: VariableId) -> &mut Variable<'a> {
        &mut self.variables[id]
    }

    /// The variables of a scope (`scope.variables.each_value`).
    pub fn scope_variables(&self, id: ScopeId) -> impl Iterator<Item = &Variable<'a>> + '_ {
        self.scopes[id]
            .variable_ids()
            .map(move |vid| &self.variables[vid])
    }

    /// The variables of a scope given by reference (the hooks' argument).
    pub fn scope_variables_of<'t>(
        &'t self,
        scope: &'t Scope<'a>,
    ) -> impl Iterator<Item = &'t Variable<'a>> + 't {
        scope.variable_ids().map(move |vid| &self.variables[vid])
    }

    /// The ids of the scopes on the stack, outermost first.
    pub fn scope_stack(&self) -> &[ScopeId] {
        &self.scope_stack
    }

    pub fn push_scope(
        &mut self,
        scope_node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> ScopeId {
        let id = self.scopes.len();
        self.scopes.push(Scope::new(scope_node));
        hooks.scope_hook(Hook::BeforeEnteringScope, id, self);
        self.scope_stack.push(id);
        hooks.scope_hook(Hook::AfterEnteringScope, id, self);
        id
    }

    pub fn pop_scope(&mut self, hooks: &mut dyn HookReceiver<'a>) -> Option<ScopeId> {
        let id = self.current_scope_id()?;
        hooks.scope_hook(Hook::BeforeLeavingScope, id, self);
        self.scope_stack.pop();
        hooks.scope_hook(Hook::AfterLeavingScope, id, self);
        Some(id)
    }

    pub fn current_scope_id(&self) -> Option<ScopeId> {
        self.scope_stack.last().copied()
    }

    pub fn current_scope(&self) -> Option<&Scope<'a>> {
        self.current_scope_id().map(|id| &self.scopes[id])
    }

    pub fn current_scope_level(&self) -> usize {
        self.scope_stack.len()
    }

    pub fn declare_variable(
        &mut self,
        name: &'a str,
        node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> Option<VariableId> {
        let scope_id = self.current_scope_id()?;
        let id = self.variables.len();
        let scope_node = self.scopes[scope_id].node;
        self.variables
            .push(Variable::new(id, name, node, scope_id, scope_node));
        hooks.variable_hook(Hook::BeforeDeclaringVariable, id, self);
        self.scopes[scope_id].set_variable(name, id);
        hooks.variable_hook(Hook::AfterDeclaringVariable, id, self);
        Some(id)
    }

    pub fn assign_to_variable(&mut self, name: &str, node: Node<'a>) {
        // RuboCop raises for an undeclared variable; that can't happen with
        // a valid AST, so just ignore it.
        let Some(id) = self.find_variable(name) else {
            return;
        };
        self.mark_variable_as_captured_by_block_if_so(id);
        self.variables[id].assign(node);
    }

    pub fn reference_variable(&mut self, name: &str, node: Node<'a>) {
        // An `lvar` node means the variable was declared, even if we haven't
        // seen its `lvasgn` or `arg` node (`foo = 1 unless foo`). Declaring
        // it here would make the `lvar` its declaration node, so skip it.
        let Some(id) = self.find_variable(name) else {
            return;
        };
        self.mark_variable_as_captured_by_block_if_so(id);
        self.variables[id].reference(node);
    }

    pub fn find_variable(&self, name: &str) -> Option<VariableId> {
        for &scope_id in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_id];
            if let Some(id) = scope.variable_id(name) {
                return Some(id);
            }
            // Only block scope allows referencing outer scope variables.
            if !scope.node.is_any_block() {
                return None;
            }
        }
        None
    }

    pub fn is_variable_exist(&self, name: &str) -> bool {
        self.find_variable(name).is_some()
    }

    pub fn accessible_variables(&self) -> Vec<VariableId> {
        let mut variables = Vec::new();
        for &scope_id in self.scope_stack.iter().rev() {
            let scope = &self.scopes[scope_id];
            variables.extend(scope.variable_ids());
            if !scope.node.is_any_block() {
                break;
            }
        }
        variables
    }

    fn mark_variable_as_captured_by_block_if_so(&mut self, id: VariableId) {
        let Some(current) = self.current_scope_id() else {
            return;
        };
        let current_node = self.scopes[current].node;
        if !current_node.is_any_block() {
            return;
        }
        if self.scopes[self.variables[id].scope].node == current_node {
            return;
        }
        self.variables[id].capture_with_block();
    }
}
