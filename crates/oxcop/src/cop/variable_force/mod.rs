//! Port of `lib/rubocop/cop/variable_force.rb` (and `force.rb`): a force
//! that tracks local variables and scopes. Cops join it with
//! `Cop::joins_variable_force` and implement the hooks they need
//! (`before_entering_scope`, `after_leaving_scope`, ...). The Commissioner
//! runs it once per investigation, before the AST walk, with the cops of
//! that investigation that join it (`Team.forces_for`).

pub mod assignment;
pub mod branch;
pub mod branchable;
pub mod reference;
pub mod scope;
pub mod variable;
pub mod variable_table;

use std::collections::HashSet;

use crate::ast::{Node, NodeId, NodeType};
use crate::cop::{Cop, Investigation};

pub use assignment::Assignment;
pub use reference::Reference;
pub use scope::Scope;
pub use variable::Variable;
pub use variable_table::{Hook, HookReceiver, VariableTable};

pub type ScopeId = usize;
pub type VariableId = usize;

pub const VARIABLE_ASSIGNMENT_TYPE: NodeType = NodeType::Lvasgn;
pub const REGEXP_NAMED_CAPTURE_TYPE: NodeType = NodeType::MatchWithLvasgn;
pub const PATTERN_MATCH_VARIABLE_TYPE: NodeType = NodeType::MatchVar;
pub const VARIABLE_ASSIGNMENT_TYPES: &[NodeType] = &[
    VARIABLE_ASSIGNMENT_TYPE,
    REGEXP_NAMED_CAPTURE_TYPE,
    PATTERN_MATCH_VARIABLE_TYPE,
];

pub const ARGUMENT_DECLARATION_TYPES: &[NodeType] = &[
    NodeType::Arg,
    NodeType::Optarg,
    NodeType::Restarg,
    NodeType::Kwarg,
    NodeType::Kwoptarg,
    NodeType::Kwrestarg,
    // This doesn't mean block argument, it's block-pass (&block).
    NodeType::Blockarg,
    // This means block local variable (obj.each { |arg; this| }).
    NodeType::Shadowarg,
];

pub const LOGICAL_OPERATOR_ASSIGNMENT_TYPES: &[NodeType] = &[NodeType::OrAsgn, NodeType::AndAsgn];
pub const OPERATOR_ASSIGNMENT_TYPES: &[NodeType] =
    &[NodeType::OrAsgn, NodeType::AndAsgn, NodeType::OpAsgn];

pub const MULTIPLE_ASSIGNMENT_TYPE: NodeType = NodeType::Masgn;
pub const REST_ASSIGNMENT_TYPE: NodeType = NodeType::Splat;

pub const VARIABLE_REFERENCE_TYPE: NodeType = NodeType::Lvar;

pub const POST_CONDITION_LOOP_TYPES: &[NodeType] = &[NodeType::WhilePost, NodeType::UntilPost];
pub const LOOP_TYPES: &[NodeType] = &[
    NodeType::WhilePost,
    NodeType::UntilPost,
    NodeType::While,
    NodeType::Until,
    NodeType::For,
];

pub const RESCUE_TYPE: NodeType = NodeType::Rescue;

pub const ZERO_ARITY_SUPER_TYPE: NodeType = NodeType::Zsuper;

pub const TWISTED_SCOPE_TYPES: &[NodeType] = &[
    NodeType::Block,
    NodeType::Numblock,
    NodeType::Itblock,
    NodeType::Class,
    NodeType::Sclass,
    NodeType::Defs,
    NodeType::Module,
];
pub const SCOPE_TYPES: &[NodeType] = &[
    NodeType::Block,
    NodeType::Numblock,
    NodeType::Itblock,
    NodeType::Class,
    NodeType::Sclass,
    NodeType::Defs,
    NodeType::Module,
    NodeType::Def,
];

pub const SEND_TYPE: NodeType = NodeType::Send;

pub const BRANCH_NODES: &[NodeType] = &[
    NodeType::If,
    NodeType::Case,
    NodeType::CaseMatch,
    NodeType::Rescue,
];

enum DescendantReference<'a> {
    /// `VariableReference`
    Variable(&'a str),
    /// `AssignmentReference`
    Assignment(Node<'a>),
}

/// The force: the variable table and the nodes already scanned.
pub struct VariableForce<'a> {
    pub variable_table: VariableTable<'a>,
    scanned_nodes: HashSet<NodeId>,
}

impl Default for VariableForce<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> VariableForce<'a> {
    pub fn new() -> Self {
        Self {
            variable_table: VariableTable::new(),
            scanned_nodes: HashSet::new(),
        }
    }

    /// Starting point.
    pub fn investigate(&mut self, root_node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) {
        self.variable_table.push_scope(root_node, hooks);
        self.process_node(root_node, hooks);
        self.variable_table.pop_scope(hooks);
    }

    pub fn process_node(&mut self, node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) {
        let skip_children = self.dispatch(node, hooks);
        if !skip_children {
            self.process_children(node, hooks);
        }
    }

    /// `node_handler_method_name` and the handler call; returns whether
    /// the handler asked to skip the children.
    fn dispatch(&mut self, node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) -> bool {
        use NodeType::*;
        match node.ty() {
            Lvasgn => self.process_variable_assignment(node, hooks),
            MatchWithLvasgn => self.process_regexp_named_captures(node, hooks),
            MatchVar => self.process_pattern_match_variable(node, hooks),
            Masgn => self.process_variable_multiple_assignment(node, hooks),
            Lvar => {
                self.process_variable_referencing(node);
                false
            }
            Rescue => self.process_rescue(node, hooks),
            Zsuper => {
                self.process_zero_arity_super(node);
                false
            }
            Send => {
                self.process_send(node);
                false
            }
            Arg | Optarg | Restarg | Kwarg | Kwoptarg | Kwrestarg | Blockarg | Shadowarg => {
                self.process_variable_declaration(node, hooks);
                false
            }
            OrAsgn | AndAsgn | OpAsgn => self.process_variable_operator_assignment(node, hooks),
            WhilePost | UntilPost | While | Until | For => self.process_loop(node, hooks),
            Block | Numblock | Itblock | Class | Sclass | Defs | Module | Def => {
                self.process_scope(node, hooks)
            }
            _ => false,
        }
    }

    /// This is called for each scope recursively.
    fn inspect_variables_in_scope(
        &mut self,
        scope_node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) {
        self.variable_table.push_scope(scope_node, hooks);
        self.process_children(scope_node, hooks);
        self.variable_table.pop_scope(hooks);
    }

    fn process_children(&mut self, origin_node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) {
        for child_node in origin_node.child_nodes() {
            if self.is_scanned_node(child_node) {
                continue;
            }
            self.process_node(child_node, hooks);
        }
    }

    fn process_variable_declaration(&mut self, node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) {
        // restarg and kwrestarg would have no name:
        //
        //   def initialize(*)
        //   end
        let Some(variable_name) = node.child_sym(0) else {
            return;
        };
        self.variable_table
            .declare_variable(variable_name, node, hooks);
    }

    fn process_variable_assignment(
        &mut self,
        node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> bool {
        let Some(name) = node.child_sym(0) else {
            return false;
        };

        if !self.variable_table.is_variable_exist(name) {
            self.variable_table.declare_variable(name, node, hooks);
        }

        // Need to scan rhs before assignment so that we can mark previous
        // assignments as referenced if rhs has referencing to the variable
        // itself like:
        //
        //   foo = 1
        //   foo = foo + 1
        self.process_children(node, hooks);

        self.variable_table.assign_to_variable(name, node);

        true
    }

    fn process_regexp_named_captures(
        &mut self,
        node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> bool {
        let (Some(regexp_node), Some(rhs_node)) = (node.child(0), node.child(1)) else {
            return false;
        };
        let variable_names = regexp_captured_names(regexp_node);

        for name in &variable_names {
            if self.variable_table.is_variable_exist(name) {
                continue;
            }
            // Names must outlive the AST; intern them in the regexp source.
            if let Some(name) = intern_name(regexp_node, name) {
                self.variable_table.declare_variable(name, node, hooks);
            }
        }

        self.process_node(rhs_node, hooks);
        self.process_node(regexp_node, hooks);

        for name in &variable_names {
            self.variable_table.assign_to_variable(name, node);
        }

        true
    }

    fn process_pattern_match_variable(
        &mut self,
        node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> bool {
        if let Some(name) = node.child_sym(0)
            && !self.variable_table.is_variable_exist(name)
        {
            self.variable_table.declare_variable(name, node, hooks);
        }
        true
    }

    fn process_variable_operator_assignment(
        &mut self,
        node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> bool {
        let Some(asgn_node) = node.lhs() else {
            return false;
        };
        if !asgn_node.is(NodeType::Lvasgn) {
            return false;
        }
        let Some(name) = asgn_node.child_sym(0) else {
            return false;
        };

        if !self.variable_table.is_variable_exist(name) {
            self.variable_table.declare_variable(name, asgn_node, hooks);
        }

        // `foo += foo = 2` is `foo = foo + (foo = 2)`, so at operator
        // assignment node, we need to reference the variable before
        // processing rhs nodes.
        self.variable_table.reference_variable(name, node);
        if let Some(rhs) = node.rhs() {
            self.process_node(rhs, hooks);
        }
        self.variable_table.assign_to_variable(name, asgn_node);

        true
    }

    fn process_variable_multiple_assignment(
        &mut self,
        node: Node<'a>,
        hooks: &mut dyn HookReceiver<'a>,
    ) -> bool {
        if let Some(rhs_node) = node.child(1) {
            self.process_node(rhs_node, hooks);
        }
        if let Some(lhs_node) = node.child(0) {
            self.process_node(lhs_node, hooks);
        }
        true
    }

    fn process_variable_referencing(&mut self, node: Node<'a>) {
        if let Some(name) = node.child_sym(0) {
            self.variable_table.reference_variable(name, node);
        }
    }

    fn process_loop(&mut self, node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) -> bool {
        if node.is_post_condition_loop() {
            // See the comment at the end of `variable_force.rb`: the body
            // runs before the condition.
            if let Some(body_node) = node.child(1) {
                self.process_node(body_node, hooks);
            }
            if let Some(condition_node) = node.child(0) {
                self.process_node(condition_node, hooks);
            }
        } else if node.is(NodeType::For) {
            // In `for item in items` the rightmost expression is evaluated
            // first.
            if let Some(collection) = node.child(1) {
                self.process_node(collection, hooks);
            }
            if let Some(variable) = node.child(0) {
                self.process_node(variable, hooks);
            }
            if let Some(body) = node.child(2) {
                self.process_node(body, hooks);
            }
        } else {
            self.process_children(node, hooks);
        }

        self.mark_assignments_as_referenced_in_loop(node);

        true
    }

    fn process_rescue(&mut self, node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) -> bool {
        let contain_retry = node
            .child_nodes()
            .filter(|child| child.is(NodeType::Resbody))
            .any(|resbody_node| resbody_node.descendants().any(|d| d.is(NodeType::Retry)));

        // Treat begin..rescue..end with retry as a loop.
        if contain_retry {
            self.process_loop(node, hooks)
        } else {
            false
        }
    }

    fn process_zero_arity_super(&mut self, node: Node<'a>) {
        for id in self.variable_table.accessible_variables() {
            if !self.variable_table.variable(id).is_method_argument() {
                continue;
            }
            self.variable_table.variable_mut(id).reference(node);
        }
    }

    fn process_scope(&mut self, node: Node<'a>, hooks: &mut dyn HookReceiver<'a>) -> bool {
        if TWISTED_SCOPE_TYPES.contains(&node.ty()) {
            // See the comment at the end of `variable_force.rb`: e.g. the
            // send node of a block belongs to the outer scope.
            for twisted_node in twisted_nodes(node) {
                self.process_node(twisted_node, hooks);
                self.scanned_nodes.insert(twisted_node.id());
            }
        }

        self.inspect_variables_in_scope(node, hooks);
        true
    }

    fn process_send(&mut self, node: Node<'a>) {
        if node.child_sym(1) != Some("binding") {
            return;
        }
        if let Some(args) = node.child(2)
            && !args.children().is_empty()
        {
            return;
        }

        for id in self.variable_table.accessible_variables() {
            self.variable_table.variable_mut(id).reference(node);
        }
    }

    /// Mark last assignments which are referenced in the same loop as
    /// referenced by ignoring AST order since they would be referenced in
    /// next iteration.
    fn mark_assignments_as_referenced_in_loop(&mut self, node: Node<'a>) {
        let (referenced_variable_names_in_loop, assignment_nodes_in_loop) =
            find_variables_in_loop(node);

        for name in referenced_variable_names_in_loop {
            // Non related references which are caught in the above scan
            // would be skipped here.
            let Some(id) = self.variable_table.find_variable(name) else {
                continue;
            };
            let variable = self.variable_table.variable_mut(id);
            let loop_assignments: Vec<usize> = variable
                .assignments
                .iter()
                .enumerate()
                .filter(|(_, a)| assignment_nodes_in_loop.contains(&a.node.id()))
                .map(|(i, _)| i)
                .collect();
            if loop_assignments.is_empty() {
                continue;
            }

            reference_assignments(variable, &loop_assignments, node);
        }
    }

    fn is_scanned_node(&self, node: Node<'_>) -> bool {
        self.scanned_nodes.contains(&node.id())
    }
}

fn twisted_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    let mut twisted_nodes: Vec<Node<'_>> = node.child(0).into_iter().collect();
    if node.is(NodeType::Class)
        && let Some(superclass) = node.child(1)
    {
        twisted_nodes.push(superclass);
    }
    twisted_nodes
}

/// `node.to_regexp.named_captures.keys`, in the order of their groups.
fn regexp_captured_names(node: Node<'_>) -> Vec<String> {
    let mut source = String::new();
    let mut options = onig::RegexOptions::REGEX_OPTION_NONE;
    for child in node.child_nodes() {
        match child.ty() {
            NodeType::Str => {
                if let Some(s) = child.str_content() {
                    source.push_str(s);
                }
            }
            NodeType::Regopt => {
                for opt in child.children().iter().filter_map(|c| c.as_sym()) {
                    match opt {
                        "i" => options |= onig::RegexOptions::REGEX_OPTION_IGNORECASE,
                        "x" => options |= onig::RegexOptions::REGEX_OPTION_EXTEND,
                        "m" => options |= onig::RegexOptions::REGEX_OPTION_MULTILINE,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let Ok(regex) = onig::Regex::with_options(&source, options, onig::Syntax::ruby()) else {
        return Vec::new();
    };
    let mut names: Vec<(u32, String)> = Vec::new();
    regex.foreach_name(|name, groups| {
        names.push((groups.iter().copied().min().unwrap_or(0), name.to_owned()));
        true
    });
    names.sort();
    names.into_iter().map(|(_, name)| name).collect()
}

/// A `&'a str` for a capture name, found in the source of the regexp.
fn intern_name<'a>(regexp_node: Node<'a>, name: &str) -> Option<&'a str> {
    let source = regexp_node.source();
    let index = source
        .find(&format!("<{name}>"))
        .or_else(|| source.find(&format!("'{name}'")))?;
    Some(&source[index + 1..index + 1 + name.len()])
}

fn find_variables_in_loop(loop_node: Node<'_>) -> (Vec<&str>, HashSet<NodeId>) {
    let mut referenced_variable_names_in_loop = Vec::new();
    let mut assignment_nodes_in_loop = HashSet::new();

    // #each_descendant does not consider scope, but we don't need to care
    // about it here.
    for node in loop_node.descendants() {
        match descendant_reference(node) {
            Some(DescendantReference::Assignment(node)) => {
                assignment_nodes_in_loop.insert(node.id());
            }
            Some(DescendantReference::Variable(name)) => {
                referenced_variable_names_in_loop.push(name);
            }
            None => {}
        }
    }

    (referenced_variable_names_in_loop, assignment_nodes_in_loop)
}

fn descendant_reference(node: Node<'_>) -> Option<DescendantReference<'_>> {
    match node.ty() {
        NodeType::Lvar => node.child_sym(0).map(DescendantReference::Variable),
        NodeType::Lvasgn => Some(DescendantReference::Assignment(node)),
        NodeType::OrAsgn | NodeType::AndAsgn | NodeType::OpAsgn => {
            let lhs = node.lhs()?;
            if lhs.is(NodeType::Lvasgn) {
                lhs.child_sym(0).map(DescendantReference::Variable)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn reference_assignments<'a>(
    variable: &mut Variable<'a>,
    loop_assignments: &[usize],
    loop_node: Node<'a>,
) {
    // If inside a branching statement, mark all as referenced. Otherwise,
    // mark only the last assignment as referenced. Note that `rescue` must
    // be considered as branching because of the `retry` keyword.
    for &i in loop_assignments {
        let assignment = &mut variable.assignments[i];
        if assignment
            .node
            .ancestors()
            .any(|a| BRANCH_NODES.contains(&a.ty()))
        {
            assignment.reference(loop_node);
        }
    }
    if let Some(&last) = loop_assignments.last() {
        variable.assignments[last].reference(loop_node);
    }
}

// ---- wiring into the Commissioner (`Team.forces_for`, `Force#run_hook`) --

/// Runs the force over `root` for the cops that join it.
pub fn investigate_cops<'a>(
    cops: &mut [Box<dyn Cop>],
    cxs: &mut [Investigation<'a, '_>],
    root: Node<'a>,
) {
    let joining: Vec<usize> = cops
        .iter()
        .enumerate()
        .filter(|(_, cop)| cop.joins_variable_force())
        .map(|(i, _)| i)
        .collect();
    if joining.is_empty() {
        return;
    }
    let mut hooks = CopHooks { cops, cxs, joining };
    VariableForce::new().investigate(root, &mut hooks);
}

struct CopHooks<'c, 'a, 's> {
    cops: &'c mut [Box<dyn Cop>],
    cxs: &'c mut [Investigation<'a, 's>],
    joining: Vec<usize>,
}

impl<'a> HookReceiver<'a> for CopHooks<'_, 'a, '_> {
    fn scope_hook(&mut self, hook: Hook, scope: ScopeId, table: &VariableTable<'a>) {
        let scope = table.scope(scope);
        for &i in &self.joining {
            let (cop, cx) = (&mut self.cops[i], &mut self.cxs[i]);
            match hook {
                Hook::BeforeEnteringScope => cop.before_entering_scope(cx, scope, table),
                Hook::AfterEnteringScope => cop.after_entering_scope(cx, scope, table),
                Hook::BeforeLeavingScope => cop.before_leaving_scope(cx, scope, table),
                Hook::AfterLeavingScope => cop.after_leaving_scope(cx, scope, table),
                _ => {}
            }
        }
    }

    fn variable_hook(&mut self, hook: Hook, variable: VariableId, table: &VariableTable<'a>) {
        let variable = table.variable(variable);
        for &i in &self.joining {
            let (cop, cx) = (&mut self.cops[i], &mut self.cxs[i]);
            match hook {
                Hook::BeforeDeclaringVariable => cop.before_declaring_variable(cx, variable, table),
                Hook::AfterDeclaringVariable => cop.after_declaring_variable(cx, variable, table),
                _ => {}
            }
        }
    }
}
