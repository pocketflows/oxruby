//! Port of `lib/rubocop/cop/style/parallel_assignment.rb`.

use crate::ast::{Child, MapKind, Node, NodeType};
use crate::cop::mixin::alignment::{indentation, offset};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::mixin::rescue_node::{modifier_locations, rescue_modifier};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;
use crate::regex;

const MSG: &str = "Do not use parallel assignment.";

/// A right-hand element, or the `s(:send, s(:self), var)` node that
/// `add_self_to_getters` builds for an implicit-self getter.
#[derive(Clone, Copy)]
enum Rhs<'a> {
    Node(Node<'a>),
    SelfGetter(&'a str),
}

#[derive(Default)]
pub struct ParallelAssignment {
    ignored: IgnoredNodes,
    modifier_locations: Option<Vec<Range>>,
}

impl Cop for ParallelAssignment {
    fn name(&self) -> &'static str {
        "Style/ParallelAssignment"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Masgn]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.modifier_locations = None;
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.part_of(node) {
            return;
        }
        let Some(mut rhs) = node.child(1) else {
            return;
        };
        if rhs.is(NodeType::Rescue) {
            let Some(body) = rhs.body() else {
                return;
            };
            rhs = body;
        }
        let rhs_elements: Vec<Node<'a>> = rhs.child_nodes().collect();
        let assignments = mlhs_assignments(node.child(0).unwrap());
        if allowed_lhs(&assignments)
            || allowed_rhs(rhs, &rhs_elements)
            || allowed_masign(&assignments, &rhs_elements)
            || contains_heredoc(rhs)
        {
            return;
        }
        let range = node.source_range().begin().join(rhs.source_range().end());
        let locations = self
            .modifier_locations
            .get_or_insert_with(|| modifier_locations(cx.processed_source))
            .clone();
        let pairs: Vec<(Node<'a>, Rhs<'a>)> = assignments
            .iter()
            .copied()
            .zip(rhs_elements.iter().map(|e| Rhs::Node(*e)))
            .collect();
        let order: Vec<(Node<'a>, Node<'a>)> = tsort(&pairs)
            .unwrap_or_default()
            .into_iter()
            .map(|(l, r)| match r {
                Rhs::Node(r) => (l, r),
                Rhs::SelfGetter(_) => unreachable!(),
            })
            .collect();
        let (correction_range, correction) =
            assignment_correction(cx, &locations, node, rhs, &order);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(correction_range, &correction);
        });
        self.ignored.ignore(node);
    }
}

/// `MlhsNode#assignments`.
fn mlhs_assignments(mlhs: Node<'_>) -> Vec<Node<'_>> {
    mlhs.child_nodes()
        .flat_map(|node| {
            if node.is(NodeType::Splat) {
                vec![node.child_nodes().next().unwrap_or(node)]
            } else if node.is(NodeType::Mlhs) {
                mlhs_assignments(node)
            } else {
                vec![node]
            }
        })
        .collect()
}

fn allowed_lhs(elements: &[Node<'_>]) -> bool {
    elements.len() == 1 || elements.iter().any(|e| e.is(NodeType::Splat))
}

fn allowed_rhs(node: Node<'_>, elements: &[Node<'_>]) -> bool {
    !node.is(NodeType::Array) || elements.iter().any(|e| e.is(NodeType::Splat))
}

fn allowed_masign(lhs_elements: &[Node<'_>], rhs_elements: &[Node<'_>]) -> bool {
    if lhs_elements.len() != rhs_elements.len() {
        return true;
    }
    let pairs: Vec<(Node<'_>, Rhs<'_>)> = lhs_elements
        .iter()
        .copied()
        .zip(rhs_elements.iter().map(|e| add_self_to_getter(*e)))
        .collect();
    tsort(&pairs).is_none()
}

/// `implicit_self_getter?`: `(send nil? $_)`.
fn add_self_to_getter(e: Node<'_>) -> Rhs<'_> {
    if e.is_send() && e.receiver().is_none() && e.children().len() == 2 {
        Rhs::SelfGetter(e.method_name().unwrap_or(""))
    } else {
        Rhs::Node(e)
    }
}

fn contains_heredoc(node: Node<'_>) -> bool {
    node.descendants().any(|d| d.is_any_str() && d.is_heredoc())
}

fn same_rhs(a: Rhs<'_>, b: Rhs<'_>) -> bool {
    match (a, b) {
        (Rhs::Node(a), Rhs::Node(b)) => a.same_as(b),
        (Rhs::SelfGetter(a), Rhs::SelfGetter(b)) => a == b,
        _ => false,
    }
}

fn same_pair(a: &(Node<'_>, Rhs<'_>), b: &(Node<'_>, Rhs<'_>)) -> bool {
    a.0.same_as(b.0) && same_rhs(a.1, b.1)
}

/// `AssignmentSorter#tsort` (Kahn's algorithm). The Ruby hash is keyed by
/// structurally equal pairs, so equal pairs collapse into one entry.
fn tsort<'a>(assignments: &[(Node<'a>, Rhs<'a>)]) -> Option<Vec<(Node<'a>, Rhs<'a>)>> {
    // `@assignments.to_h`: a later equal key replaces the value in place.
    let mut dependencies: Vec<((Node<'a>, Rhs<'a>), Vec<(Node<'a>, Rhs<'a>)>)> = Vec::new();
    for assignment in assignments {
        let edges = dependencies_for_assignment(assignments, assignment);
        match dependencies.iter_mut().find(|(k, _)| same_pair(k, assignment)) {
            Some(entry) => entry.1 = edges,
            None => dependencies.push((*assignment, edges)),
        }
    }
    let mut result = Vec::new();
    while let Some(pos) = dependencies.iter().position(|(_, edges)| edges.is_empty()) {
        let (matched, _) = dependencies.remove(pos);
        result.push(matched);
        for (_, edges) in dependencies.iter_mut() {
            edges.retain(|e| !same_pair(e, &matched));
        }
    }
    // Cyclic dependency
    dependencies.is_empty().then_some(result)
}

fn dependencies_for_assignment<'a>(
    assignments: &[(Node<'a>, Rhs<'a>)],
    assignment: &(Node<'a>, Rhs<'a>),
) -> Vec<(Node<'a>, Rhs<'a>)> {
    assignments
        .iter()
        .filter(|other| !same_pair(other, assignment) && dependency(assignment.0, other.1))
        .copied()
        .collect()
}

/// `var_name`: `{(casgn _ $_) (_ $_)}`.
fn var_name<'a>(lhs: Node<'a>) -> Option<&'a Child> {
    if lhs.is(NodeType::Casgn) {
        return lhs.children().get(1);
    }
    match lhs.children() {
        [only] => Some(only),
        _ => None,
    }
}

fn dependency(lhs: Node<'_>, rhs: Rhs<'_>) -> bool {
    uses_var(rhs, var_name(lhs))
        || (lhs.is_send() && lhs.is_assignment_method() && accesses(rhs, lhs))
}

/// `uses_var?`: `{({lvar ivar cvar gvar} %) (const _ %)}` anywhere in `rhs`.
fn uses_var(rhs: Rhs<'_>, name: Option<&Child>) -> bool {
    let Rhs::Node(rhs) = rhs else {
        return false;
    };
    rhs.each_node().any(|n| {
        let child = if n.is_variable() {
            match n.children() {
                [only] => Some(only),
                _ => None,
            }
        } else if n.is(NodeType::Const) && n.children().len() == 2 {
            n.children().get(1)
        } else {
            None
        };
        match (child, name) {
            (Some(c), Some(name)) => c == name,
            (Some(c), None) => c.is_nil(),
            _ => false,
        }
    })
}

/// `matching_calls(rhs, receiver, method)`: argument lists of
/// `(send %1 %2 $...)` anywhere in `rhs`.
fn matching_calls<'a>(rhs: Rhs<'a>, receiver: Option<Node<'_>>, method: &str) -> Vec<Vec<Node<'a>>> {
    match rhs {
        Rhs::SelfGetter(name) => {
            let receiver_is_self = receiver.is_some_and(|r| r.is(NodeType::SelfNode));
            if receiver_is_self && name == method {
                vec![Vec::new()]
            } else {
                Vec::new()
            }
        }
        Rhs::Node(rhs) => rhs
            .each_node()
            .filter(|n| {
                n.is_send()
                    && n.method_name() == Some(method)
                    && match (n.receiver(), receiver) {
                        (None, None) => true,
                        (Some(a), Some(b)) => a.same_as(b),
                        _ => false,
                    }
            })
            .map(|n| n.arguments())
            .collect(),
    }
}

fn accesses(rhs: Rhs<'_>, lhs: Node<'_>) -> bool {
    if lhs.is_method("[]=") {
        let lhs_args = lhs.arguments();
        matching_calls(rhs, lhs.receiver(), "[]").iter().any(|args| {
            args.len() == lhs_args.len() && args.iter().zip(&lhs_args).all(|(a, b)| a.same_as(*b))
        })
    } else {
        let name = lhs.method_name().unwrap_or("");
        let mut chars = name.chars();
        chars.next_back();
        !matching_calls(rhs, lhs.receiver(), chars.as_str()).is_empty()
    }
}

fn modifier_statement(node: Option<Node<'_>>) -> bool {
    node.is_some_and(|n| n.is_basic_conditional() && n.is_modifier_form())
}

/// `assignment_corrector(node, rhs, order)`: the correction range and text.
fn assignment_correction(
    cx: &Investigation<'_, '_>,
    modifier_locations: &[Range],
    node: Node<'_>,
    rhs: Node<'_>,
    order: &[(Node<'_>, Node<'_>)],
) -> (Range, String) {
    let modifier = match node.parent() {
        Some(parent) if parent.is(NodeType::Rescue) => parent.child(1),
        _ => rhs.parent().and_then(|p| p.child(1)),
    };
    let assignment: Vec<String> = order
        .iter()
        .map(|(lhs, rhs)| format!("{} = {}", lhs.source(), source(*rhs)))
        .collect();
    let indentation = indentation(cx, node);
    let offset = offset(node);
    if modifier_statement(node.parent()) {
        let parent = node.parent().unwrap();
        let modifier_range = parent
            .loc()
            .keyword
            .unwrap()
            .join(parent.source_range().end());
        let correction = format!(
            "{}\n{indentation}{}\n{offset}end",
            parent.range_source(modifier_range),
            assignment.join(&format!("\n{indentation}"))
        );
        (parent.source_range(), correction)
    } else if let Some(modifier) =
        modifier.filter(|m| rescue_modifier(modifier_locations, *m))
    {
        let rescue_result = modifier.child(2).map_or("", |r| r.source());
        let rescue_parent = rhs.parent().unwrap();
        let range_node = rescue_parent.parent().unwrap();
        let correction = if range_node.parent().is_some_and(|p| p.is(NodeType::Def)) {
            format!(
                "{}\nrescue\n{offset}{rescue_result}",
                assignment.join(&format!("\n{offset}"))
            )
        } else {
            format!(
                "begin\n{indentation}{}\n{offset}rescue\n{indentation}{rescue_result}\n{offset}end",
                assignment.join(&format!("\n{indentation}"))
            )
        };
        (range_node.source_range(), correction)
    } else {
        (node.source_range(), assignment.join(&format!("\n{offset}")))
    }
}

/// `GenericCorrector#source(node, loc)`.
fn source(node: Node<'_>) -> String {
    // `__FILE__` is a `str` whose map has no `begin` at all.
    if node.is(NodeType::Str)
        && node.loc().kind == MapKind::Collection
        && node.loc().begin.is_none()
    {
        // `%w` elements have no per-element delimiter.
        let value = node.str_content().unwrap_or("");
        let escaped = regex!(r"[\\']").replace_all(value, |caps: &onig::Captures<'_>| {
            format!("\\{}", caps.at(0).unwrap_or(""))
        });
        format!("'{escaped}'")
    } else if node.is(NodeType::Sym) && node.loc().begin.is_none() {
        symbol_inspect(node.sym_value().unwrap_or(""))
    } else {
        node.source().to_owned()
    }
}

/// `Symbol#inspect`.
fn symbol_inspect(s: &str) -> String {
    if regex!(
        r"\A(?:[A-Za-z_][A-Za-z0-9_]*[?!=]?|@@?[A-Za-z_][A-Za-z0-9_]*|\$[A-Za-z_][A-Za-z0-9_]*|\[\]=?|[+\-*/%<>!~^&|]|\*\*|<=>|==|===|=~|!=|!~|<<|>>|<=|>=|[+-]@)\z"
    )
    .find(s)
    .is_some()
    {
        format!(":{s}")
    } else {
        format!(":{}", ruby_inspect(s))
    }
}
