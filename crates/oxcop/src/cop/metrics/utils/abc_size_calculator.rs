//! Port of `lib/rubocop/cop/metrics/utils/abc_size_calculator.rb`.
//!
//! > ABC is .. a software size metric .. computed by counting the number of
//! > assignments, branches and conditions for a section of code.
//! > <http://c2.com/cgi/wiki?AbcMetric>
//!
//! `RepeatedAttributeDiscount` is prepended upstream; its hooks are called
//! from `calculate_node` and `evaluate_branch_nodes` here.

use crate::ast::{Child, Node, NodeType};
use crate::cop::metrics::utils::iterating_block::iterating_block;
use crate::cop::metrics::utils::repeated_attribute_discount::RepeatedAttributeDiscount;
use crate::cop::metrics::utils::repeated_csend_discount::RepeatedCsendDiscount;

/// `CyclomaticComplexity::COUNTED_NODES`.
const CONDITION_NODES: &[NodeType] = &[
    NodeType::If,
    NodeType::While,
    NodeType::Until,
    NodeType::For,
    NodeType::Csend,
    NodeType::Block,
    NodeType::BlockPass,
    NodeType::Rescue,
    NodeType::When,
    NodeType::InPattern,
    NodeType::And,
    NodeType::Or,
    NodeType::OrAsgn,
    NodeType::AndAsgn,
];

pub struct AbcSizeCalculator {
    assignment: usize,
    branch: usize,
    condition: usize,
    csend: RepeatedCsendDiscount,
    attributes: Option<RepeatedAttributeDiscount>,
}

impl AbcSizeCalculator {
    /// `calculate(node, discount_repeated_attributes:)`: the score (rounded
    /// to two decimals) and the `<A, B, C>` vector.
    pub fn calculate(node: Node<'_>, discount_repeated_attributes: bool) -> (f64, String) {
        let mut calculator = Self {
            assignment: 0,
            branch: 0,
            condition: 0,
            csend: RepeatedCsendDiscount::default(),
            attributes: discount_repeated_attributes.then(RepeatedAttributeDiscount::new),
        };
        calculator.visit_depth_last(node);
        let (a, b, c) = (
            calculator.assignment as f64,
            calculator.branch as f64,
            calculator.condition as f64,
        );
        let score = round2((a * a + b * b + c * c).sqrt());
        (
            score,
            format!(
                "<{}, {}, {}>",
                calculator.assignment, calculator.branch, calculator.condition
            ),
        )
    }

    fn evaluate_branch_nodes(&mut self, node: Node<'_>) {
        if let Some(attributes) = &mut self.attributes
            && attributes.discount_repeated_attribute(node)
        {
            return;
        }
        if node.is_comparison_method() {
            self.condition += 1;
        } else {
            self.branch += 1;
            if node.is(NodeType::Csend) && !self.csend.discount_for_repeated_csend(node) {
                self.condition += 1;
            }
        }
    }

    fn evaluate_condition_node(&mut self, node: Node<'_>) {
        if else_branch(node) {
            self.condition += 1;
        }
        self.condition += 1;
    }

    fn visit_depth_last(&mut self, node: Node<'_>) {
        for child in node.child_nodes() {
            self.visit_depth_last(child);
        }
        self.calculate_node(node);
    }

    fn calculate_node(&mut self, node: Node<'_>) {
        if let Some(attributes) = &mut self.attributes {
            attributes.update_repeated_attribute(node);
        }
        if self.is_assignment(node) {
            self.assignment += 1;
        }
        if branch(node) {
            self.evaluate_branch_nodes(node);
        } else if condition(node) {
            self.evaluate_condition_node(node);
        }
    }

    fn is_assignment(&mut self, node: Node<'_>) -> bool {
        if node.is(NodeType::Masgn) || node.is_shorthand_asgn() {
            self.compound_assignment(node);
            return false;
        }
        node.is(NodeType::For)
            || (responds_to_setter_method(node) && node.loc().operator.is_some())
            || self.simple_assignment(node)
            || argument(node)
    }

    /// Methods setter cannot be detected for multiple assignments and
    /// shorthand assigns, so we'll count them here instead.
    fn compound_assignment(&mut self, node: Node<'_>) {
        let children: Vec<Node<'_>> = if node.is(NodeType::Masgn) {
            node.child(0).map(mlhs_assignments).unwrap_or_default()
        } else {
            node.child_nodes().collect()
        };
        let will_be_miscounted = children
            .iter()
            .filter(|child| responds_to_setter_method(**child) && child.loc().operator.is_none())
            .count();
        self.assignment += will_be_miscounted;
    }

    fn simple_assignment(&mut self, node: Node<'_>) -> bool {
        if !node.is_equals_asgn() {
            false
        } else if node.is(NodeType::Lvasgn) {
            self.csend.reset_on_lvasgn(node);
            capturing_variable(node.children().first())
        } else {
            true
        }
    }
}

/// `MlhsNode#assignments`.
fn mlhs_assignments(mlhs: Node<'_>) -> Vec<Node<'_>> {
    mlhs.child_nodes()
        .flat_map(|node| {
            if node.is(NodeType::Splat) {
                node.child(0).into_iter().collect()
            } else if node.is(NodeType::Mlhs) {
                mlhs_assignments(node)
            } else {
                vec![node]
            }
        })
        .collect()
}

/// Node classes including `MethodDispatchNode` (`respond_to?(:setter_method?)`).
fn responds_to_setter_method(node: Node<'_>) -> bool {
    use NodeType::*;
    matches!(
        node.ty(),
        Send | Csend | Super | Zsuper | Yield | Defined | Index | Indexasgn | Lambda
    )
}

fn else_branch(node: Node<'_>) -> bool {
    matches!(node.ty(), NodeType::Case | NodeType::If)
        && node
            .loc()
            .else_
            .is_some_and(|e| node.range_source(e) == "else")
}

fn capturing_variable(name: Option<&Child>) -> bool {
    matches!(name, Some(Child::Sym(name)) if !name.starts_with('_'))
}

fn branch(node: Node<'_>) -> bool {
    matches!(node.ty(), NodeType::Send | NodeType::Csend | NodeType::Yield)
}

fn argument(node: Node<'_>) -> bool {
    node.is_argument_type() && capturing_variable(node.children().first())
}

fn condition(node: Node<'_>) -> bool {
    if iterating_block(node) == Some(false) {
        return false;
    }
    CONDITION_NODES.contains(&node.ty())
}

/// `Float#round(2)`.
fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
