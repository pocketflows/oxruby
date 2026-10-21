//! Port of `lib/rubocop/cop/style/conditional_assignment.rb`, with its
//! `ConditionalAssignmentHelper`, `ConditionalCorrectorHelper` and the
//! `TernaryCorrector`, `IfCorrector` and `CaseCorrector` classes.
//!
//! `safe_to_correct?` is `ReparsedEquivalence#correction_parses?`: the
//! correction is applied to the source, which must still parse.

use serde_yaml_ng::Value;

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::util::parentheses;
use crate::cop::mixin::util::regexp_escape;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::{ProcessedSource, Range};

const MSG: &str = "Use the return of the conditional for variable assignment and comparison.";
const ASSIGN_TO_CONDITION_MSG: &str = "Assign variables inside of conditionals.";

const VARIABLE_ASSIGNMENT_TYPES: &[NodeType] = &[
    NodeType::Casgn,
    NodeType::Cvasgn,
    NodeType::Gvasgn,
    NodeType::Ivasgn,
    NodeType::Lvasgn,
];

static ASSIGNMENT_TYPE: NodePattern = NodePattern::new(
    "{
       casgn cvasgn gvasgn ivasgn lvasgn and_asgn or_asgn op_asgn masgn
       (send _recv {:[]= :<< :=~ :!~ :<=> #end_with_eq? :< :>} ...)
     }",
);

fn assignment_type(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "end_with_eq?" && val.sym().is_some_and(|s| s.ends_with('='))
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    ASSIGNMENT_TYPE.match_with(node, &ctx).is_some()
}

/// What the cop needs from its configuration while correcting.
#[derive(Clone, Copy)]
struct Settings {
    /// `Layout/EndAlignment`'s `EnforcedStyleAlignWith` is `keyword`.
    end_alignment_keyword: bool,
}

#[derive(Default)]
pub struct ConditionalAssignment {
    ignored: IgnoredNodes,
}

impl Cop for ConditionalAssignment {
    fn name(&self) -> &'static str {
        "Style/ConditionalAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Casgn,
            NodeType::Cvasgn,
            NodeType::Gvasgn,
            NodeType::Ivasgn,
            NodeType::Lvasgn,
            NodeType::AndAsgn,
            NodeType::OrAsgn,
            NodeType::OpAsgn,
            NodeType::Masgn,
            NodeType::Send,
            NodeType::If,
            NodeType::Case,
            NodeType::CaseMatch,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Send => {
                if assignment_type(node) {
                    self.check_assignment_to_condition(cx, node);
                }
            }
            NodeType::If => {
                if style(cx) != "assign_to_condition" || node.is_elsif() {
                    return;
                }
                let (elsif_branches, else_branch) = expand_elses(node.else_branch());
                let Some(else_branch) = else_branch else {
                    return;
                };
                let mut branches = vec![node.if_branch()];
                branches.extend(elsif_branches);
                branches.push(Some(else_branch));
                check_node(cx, node, &branches);
            }
            NodeType::Case | NodeType::CaseMatch => {
                if style(cx) != "assign_to_condition" {
                    return;
                }
                let Some(else_branch) = node.else_branch() else {
                    return;
                };
                let mut branches: Vec<Option<Node<'_>>> =
                    node.when_branches().iter().map(|w| w.body()).collect();
                branches.push(Some(else_branch));
                check_node(cx, node, &branches);
            }
            _ => {
                if self.ignored.part_of(node) {
                    return;
                }
                if node.parent().is_some_and(|p| p.is_shorthand_asgn()) {
                    return;
                }
                self.check_assignment_to_condition(cx, node);
            }
        }
    }
}

impl ConditionalAssignment {
    fn check_assignment_to_condition<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if style(cx) != "assign_inside_condition" || !assignment_rhs_exist(node) {
            return;
        }
        self.ignored.ignore(node);
        let Some(assignment) = assignment_node(node) else {
            return;
        };
        if !matches!(
            assignment.ty(),
            NodeType::If | NodeType::Case | NodeType::CaseMatch
        ) || allowed_ternary(cx, assignment)
        {
            return;
        }
        if assignment.else_branch().is_none() {
            return;
        }
        let branches: Vec<Option<Node<'_>>> = assignment
            .children()
            .iter()
            .skip(1)
            .map(|c| c.as_node().map(|id| node.ast().node(id)))
            .collect();
        if allowed_single_line(cx, &branches) {
            return;
        }
        let settings = settings(cx);
        if !safe_to_correct(cx.processed_source, node, settings) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), ASSIGN_TO_CONDITION_MSG, |corrector| {
            autocorrect(corrector, node, settings);
        });
    }
}

fn settings(cx: &Investigation<'_, '_>) -> Settings {
    Settings {
        end_alignment_keyword: cx
            .config
            .for_cop("Layout/EndAlignment")
            .get("EnforcedStyleAlignWith")
            .and_then(Value::as_str)
            == Some("keyword"),
    }
}

fn check_node(cx: &mut Investigation<'_, '_>, node: Node<'_>, branches: &[Option<Node<'_>>]) {
    if allowed_ternary(cx, node) || !allowed_statements(branches) {
        return;
    }
    if allowed_single_line(cx, branches) || correction_exceeds_line_limit(cx, node, branches) {
        return;
    }
    let settings = settings(cx);
    if !safe_to_correct(cx.processed_source, node, settings) {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
        autocorrect(corrector, node, settings);
    });
}

fn allowed_ternary(cx: &Investigation<'_, '_>, assignment: Node<'_>) -> bool {
    assignment.is_ternary() && !cx.cop_config_bool("IncludeTernaryExpressions", false)
}

fn allowed_single_line(cx: &Investigation<'_, '_>, branches: &[Option<Node<'_>>]) -> bool {
    cx.cop_config_bool("SingleLineConditionsOnly", false)
        && branches.iter().flatten().any(|b| b.is(NodeType::Begin))
}

fn assignment_node(node: Node<'_>) -> Option<Node<'_>> {
    let mut assignment = if node.is_send() {
        node.last_argument()
    } else {
        node.expression()
    }?;
    if node.parent().is_some_and(|p| p.is(NodeType::For)) {
        return None;
    }
    if assignment.is(NodeType::Begin) && assignment.children().len() == 1 {
        assignment = assignment.child(0)?;
    }
    Some(assignment)
}

fn assignment_rhs_exist(node: Node<'_>) -> bool {
    node.parent()
        .is_none_or(|p| !matches!(p.ty(), NodeType::Mlhs | NodeType::Resbody))
}

fn allowed_statements(branches: &[Option<Node<'_>>]) -> bool {
    if branches.iter().any(Option::is_none) {
        return false;
    }
    let statements: Vec<Node<'_>> = branches.iter().flatten().filter_map(|b| tail(*b)).collect();
    lhs_all_match(&statements)
        && !statements.iter().any(|s| s.is(NodeType::Masgn))
        && assignment_types_match(&statements)
}

fn lhs_all_match(branches: &[Node<'_>]) -> bool {
    let Some(first) = branches.first() else {
        return true;
    };
    let first_lhs = lhs(*first);
    branches.iter().all(|b| lhs(*b) == first_lhs)
}

fn assignment_types_match(nodes: &[Node<'_>]) -> bool {
    let Some(first) = nodes.first() else {
        return false;
    };
    assignment_type(*first) && nodes.iter().all(|n| n.ty() == first.ty())
}

fn correction_exceeds_line_limit(
    cx: &Investigation<'_, '_>,
    node: Node<'_>,
    branches: &[Option<Node<'_>>],
) -> bool {
    const COP: &str = "Layout/LineLength";
    if !cx.config.cop_enabled(COP) {
        return false;
    }
    let Some(first) = branches.first().copied().flatten().and_then(tail) else {
        return false;
    };
    let max = cx
        .config
        .for_cop(COP)
        .get("Max")
        .and_then(Value::as_i64)
        .unwrap_or(120);
    longest_line(node, &lhs(first)).chars().count() as i64 > max
}

fn longest_line(node: Node<'_>, assignment: &str) -> String {
    let escaped = regexp_escape(assignment).replace("\\ ", "\\s*");
    let regex = crate::cop::mixin::ruby_regex(&format!(r"\s*{escaped}"));
    let lines: Vec<String> = node
        .source()
        .split_inclusive('\n')
        .map(|line| {
            let line = line.strip_suffix('\n').unwrap_or(line);
            match regex.as_ref().and_then(|r| r.find(line)) {
                Some((start, end)) => format!("{}{}", &line[..start], &line[end..]),
                None => line.to_owned(),
            }
        })
        .collect();
    // `max_by(&:length)`: the first of the longest.
    let mut longest = String::new();
    let mut longest_len = None;
    for line in lines {
        let len = line.chars().count();
        if longest_len.is_none_or(|l| len > l) {
            longest_len = Some(len);
            longest = line;
        }
    }
    format!("{assignment}{longest}")
}


/// `safe_to_correct?`: `correction_parses?(node)`; a correction that raises
/// (here: one that finds the tree in an unexpected shape) is not safe.
fn safe_to_correct(ps: &ProcessedSource<'_>, node: Node<'_>, settings: Settings) -> bool {
    let source = ps.buffer_source();
    let mut corrector = Corrector::new(source.len());
    if autocorrect(&mut corrector, node, settings).is_none() || corrector.is_clobbered() {
        return false;
    }
    let corrected = corrector.rewrite(source);
    crate::prism::parse(&corrected, ps.ruby_version())
        .errors()
        .next()
        .is_none()
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, settings: Settings) -> Option<()> {
    if assignment_type(node) {
        move_assignment_inside_condition(corrector, node)
    } else {
        move_assignment_outside_condition(corrector, node, settings)
    }
}

fn move_assignment_outside_condition(
    corrector: &mut Corrector,
    node: Node<'_>,
    settings: Settings,
) -> Option<()> {
    if matches!(node.ty(), NodeType::Case | NodeType::CaseMatch) {
        case_correct(corrector, node, settings)
    } else if node.is_ternary() {
        corrector.replace(node.source_range(), ternary_correction(node)?);
        Some(())
    } else if node.is_if() || node.is_unless() {
        correct_if_branches(corrector, node, settings)
    } else {
        Some(())
    }
}

fn condition_of(node: Node<'_>) -> Option<Node<'_>> {
    if node.is_send() {
        node.last_argument()
    } else {
        node.expression()
    }
}

fn move_assignment_inside_condition(corrector: &mut Corrector, node: Node<'_>) -> Option<()> {
    let condition = condition_of(node)?;
    if ternary_condition(condition) {
        ternary_move_assignment_inside_condition(corrector, node)
    } else if matches!(condition.ty(), NodeType::Case | NodeType::CaseMatch) {
        case_move_assignment_inside_condition(corrector, node)
    } else if condition.is(NodeType::If) {
        if_move_assignment_inside_condition(corrector, node)
    } else {
        Some(())
    }
}

fn ternary_condition(node: Node<'_>) -> bool {
    node.is_ternary() || node.child(0).is_some_and(|c| c.is_ternary())
}

// ---- ConditionalAssignmentHelper ---------------------------------------------

fn expand_elses(branch: Option<Node<'_>>) -> (Vec<Option<Node<'_>>>, Option<Node<'_>>) {
    let mut elsif_branches = expand_elsif(branch);
    let else_branch = if elsif_branches.is_empty() {
        branch
    } else {
        elsif_branches.pop().flatten()
    };
    (elsif_branches, else_branch)
}

fn expand_elsif(node: Option<Node<'_>>) -> Vec<Option<Node<'_>>> {
    let mut out = Vec::new();
    let mut node = node;
    loop {
        let Some(n) = node.filter(|n| n.is(NodeType::If) && n.is_elsif()) else {
            return out;
        };
        out.push(n.if_branch());
        let else_branch = n.else_branch();
        if else_branch.is_some_and(|e| e.is(NodeType::If) && e.is_elsif()) {
            node = else_branch;
        } else {
            out.push(else_branch);
            return out;
        }
    }
}

/// `tail(branch)`: the last statement of a `begin`.
fn tail(branch: Node<'_>) -> Option<Node<'_>> {
    if branch.is(NodeType::Begin) {
        branch.child_nodes().last()
    } else {
        Some(branch)
    }
}

fn lhs(node: Node<'_>) -> String {
    match node.ty() {
        NodeType::Send => lhs_for_send(node),
        NodeType::OpAsgn | NodeType::AndAsgn | NodeType::OrAsgn => {
            let operator = match node.ty() {
                NodeType::AndAsgn => "&&",
                NodeType::OrAsgn => "||",
                _ => node.child_sym(1).unwrap_or(""),
            };
            format!(
                "{} {operator}= ",
                node.child(0).map_or("", |a| a.source())
            )
        }
        NodeType::Casgn => lhs_for_casgn(node),
        ty if VARIABLE_ASSIGNMENT_TYPES.contains(&ty) => {
            format!("{} = ", node.name().unwrap_or(""))
        }
        _ => node.source().to_owned(),
    }
}

fn indent(settings: Settings, source: &str) -> String {
    if settings.end_alignment_keyword {
        " ".repeat(source.chars().count())
    } else {
        String::new()
    }
}

fn lhs_for_send(node: Node<'_>) -> String {
    let receiver = node.receiver().map_or("", |r| r.source());
    let method_name = node.method_name().unwrap_or("");
    if method_name == "[]=" {
        let arguments = node.arguments();
        let indices: Vec<&str> = arguments[..arguments.len().saturating_sub(1)]
            .iter()
            .map(|a| a.source())
            .collect();
        format!("{receiver}[{}] = ", indices.join(", "))
    } else if node.is_setter_method() {
        format!(
            "{receiver}.{} = ",
            method_name.strip_suffix('=').unwrap_or(method_name)
        )
    } else {
        format!("{receiver} {method_name} ")
    }
}

fn lhs_for_casgn(node: Node<'_>) -> String {
    let name = node.name().unwrap_or("");
    match node.namespace() {
        None => format!("{name} = "),
        Some(ns) if ns.is(NodeType::Cbase) => format!("::{name} = "),
        Some(ns) => format!("{}::{name} = ", ns.const_name().unwrap_or_default()),
    }
}

// ---- ConditionalCorrectorHelper ----------------------------------------------

fn line_of_range(node: Node<'_>, range: Range) -> usize {
    node.ast().line_of(range.begin_pos)
}

fn remove_whitespace_in_branches(
    corrector: &mut Corrector,
    branch: Node<'_>,
    condition: Node<'_>,
    column: usize,
) -> Option<()> {
    for child in branch.each_node() {
        if !child.has_source() {
            continue;
        }
        if child.parent().is_some_and(|p| p.is(NodeType::Dstr)) {
            continue;
        }
        if let Some(white_space) = white_space_range(child, column) {
            corrector.remove(white_space);
        }
    }
    if let Some(else_loc) = condition.loc().else_ {
        let else_branch = condition.else_branch()?;
        if else_branch.first_line() != condition.first_line() {
            let size = condition.column_of(else_loc.begin_pos).checked_sub(column)?;
            corrector.remove_preceding(else_loc, size);
        }
    }
    let Some(end_loc) = condition.loc().end else {
        return Some(());
    };
    // `same_line?` is false when the last branch's `if` has no `else`.
    let last_else = condition
        .branches()
        .last()
        .copied()
        .flatten()
        .and_then(|last| last.parent())
        .and_then(|parent| parent.else_branch());
    if last_else.is_none_or(|e| e.first_line() != line_of_range(condition, end_loc)) {
        let size = condition.column_of(end_loc.begin_pos).checked_sub(column)?;
        corrector.remove_preceding(end_loc, size);
    }
    Some(())
}

fn white_space_range(node: Node<'_>, column: usize) -> Option<Range> {
    let expression = node.source_range();
    let end_pos = expression.begin_pos as isize;
    let begin_pos = end_pos - (node.column() as isize - column as isize - 2);
    if begin_pos > end_pos || begin_pos < 0 {
        return None;
    }
    let white_space = Range::new(begin_pos as usize, end_pos as usize);
    let source = node.ast().source().get(white_space.begin_pos..white_space.end_pos)?;
    source.trim().is_empty().then_some(white_space)
}

fn assignment_range(node: Node<'_>) -> Option<Range> {
    let condition = condition_of(node)?;
    Some(
        node.source_range()
            .begin()
            .join(condition.source_range().begin()),
    )
}

fn correct_if_branches(corrector: &mut Corrector, node: Node<'_>, settings: Settings) -> Option<()> {
    let branches = node.branches();
    let (first, rest) = branches.split_first()?;
    let (last, middle) = rest.split_last()?;
    let if_branch = tail((*first)?)?;
    let else_branch = tail((*last)?)?;
    let elsif_branches: Vec<Node<'_>> = middle
        .iter()
        .map(|b| b.and_then(tail))
        .collect::<Option<_>>()?;
    let lhs = lhs(if_branch);
    corrector.insert_before(node.source_range(), &lhs);
    replace_branch_assignment(corrector, if_branch)?;
    correct_branches(corrector, &elsif_branches)?;
    replace_branch_assignment(corrector, else_branch)?;
    corrector.insert_before(node.loc().end?, indent(settings, &lhs));
    Some(())
}

fn replace_branch_assignment(corrector: &mut Corrector, branch: Node<'_>) -> Option<()> {
    let assignment = branch.child_from_end(1)?;
    let source = assignment.source();
    let replacement = if assignment.is(NodeType::Array) && !assignment.is_bracketed() {
        format!("[{source}]")
    } else {
        source.to_owned()
    };
    corrector.replace(branch.source_range(), replacement);
    Some(())
}

fn correct_branches(corrector: &mut Corrector, branches: &[Node<'_>]) -> Option<()> {
    for branch in branches {
        let assignment = branch.child_from_end(1)?;
        corrector.replace(branch.source_range(), assignment.source());
    }
    Some(())
}

// ---- TernaryCorrector ----------------------------------------------------------

fn ternary_correction(node: Node<'_>) -> Option<String> {
    let if_branch = node.if_branch()?;
    let else_branch = node.else_branch()?;
    let if_rhs = if_branch.child_from_end(1)?;
    let else_rhs = else_branch.child_from_end(1)?;
    let expr = format!(
        "{} ? {} : {}",
        node.condition()?.source(),
        if_rhs.source(),
        else_rhs.source()
    );
    let expr = if if_branch.is_send() && !if_branch.is_method("[]=") {
        format!("({expr})")
    } else {
        expr
    };
    Some(format!("{}{expr}", lhs(if_branch)))
}

fn ternary_move_assignment_inside_condition(corrector: &mut Corrector, node: Node<'_>) -> Option<()> {
    let rhs = condition_of(node)?;
    let condition = if rhs.is(NodeType::Begin) && rhs.children().len() == 1 {
        rhs.child(0)?
    } else {
        rhs
    };
    let if_branch = condition.child(1)?;
    let else_branch = condition.child(2)?;
    let assignment = assignment_range(node)?;
    if parentheses(rhs) {
        corrector.remove(rhs.loc().begin?);
        corrector.remove(rhs.loc().end?);
    }
    corrector.remove(assignment);
    let source = node.range_source(assignment);
    corrector.insert_before(if_branch.source_range(), source);
    corrector.insert_before(else_branch.source_range(), source);
    Some(())
}

// ---- IfCorrector -----------------------------------------------------------------

fn if_move_assignment_inside_condition(corrector: &mut Corrector, node: Node<'_>) -> Option<()> {
    let column = node.column();
    let condition = condition_of(node)?;
    let assignment = assignment_range(node)?;
    corrector.remove(assignment);
    let source = node.range_source(assignment);
    for branch in condition.branches() {
        let branch = branch?;
        let branch_assignment = tail(branch)?;
        corrector.insert_before(branch_assignment.source_range(), source);
        remove_whitespace_in_branches(corrector, branch, condition, column)?;
        let Some(branch_else) = branch.parent()?.loc().else_ else {
            continue;
        };
        if line_of_range(node, branch_else) == condition.first_line() {
            continue;
        }
        let size = node.column_of(branch_else.begin_pos).checked_sub(column)?;
        corrector.remove_preceding(branch_else, size);
    }
    Some(())
}

// ---- CaseCorrector ----------------------------------------------------------------

fn case_correct(corrector: &mut Corrector, node: Node<'_>, settings: Settings) -> Option<()> {
    let when_branches: Vec<Node<'_>> = node
        .when_branches()
        .iter()
        .map(|w| w.body().and_then(tail))
        .collect::<Option<_>>()?;
    let else_branch = tail(node.else_branch()?)?;
    let lhs = lhs(else_branch);
    corrector.insert_before(node.source_range(), &lhs);
    correct_branches(corrector, &when_branches)?;
    replace_branch_assignment(corrector, else_branch)?;
    corrector.insert_before(node.loc().end?, indent(settings, &lhs));
    Some(())
}

fn case_move_assignment_inside_condition(corrector: &mut Corrector, node: Node<'_>) -> Option<()> {
    let column = node.column();
    let condition = condition_of(node)?;
    let assignment = assignment_range(node)?;
    corrector.remove(assignment);
    let source = node.range_source(assignment);
    let mut branches: Vec<Option<Node<'_>>> =
        condition.when_branches().iter().map(|w| w.body()).collect();
    branches.push(condition.else_branch());
    for branch in branches {
        let branch = branch?;
        let branch_assignment = tail(branch)?;
        corrector.insert_before(branch_assignment.source_range(), source);
        remove_whitespace_in_branches(corrector, branch, condition, column)?;
        let parent_keyword = branch.parent()?.loc().keyword?;
        if line_of_range(node, parent_keyword) == condition.first_line() {
            continue;
        }
        let size = node.column_of(parent_keyword.begin_pos).checked_sub(column)?;
        corrector.remove_preceding(parent_keyword, size);
    }
    Some(())
}
