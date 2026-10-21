//! Port of `lib/rubocop/cop/layout/multiline_assignment_layout.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::check_assignment::{self, extract_rhs};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::util::same_line;
use crate::cop::{Cop, Investigation};

const NEW_LINE_OFFENSE: &str =
    "Right hand side of multi-line assignment is on the same line as the assignment operator `=`.";
const SAME_LINE_OFFENSE: &str = "Right hand side of multi-line assignment is not on the same line as the assignment operator `=`.";
const BLOCK_TYPES: &[NodeType] = &[NodeType::Block, NodeType::Numblock, NodeType::Itblock];

#[derive(Default)]
pub struct MultilineAssignmentLayout {
    /// `@supported_types`, memoized for the cop's lifetime as in Ruby.
    supported_types: Option<Vec<NodeType>>,
}

impl Cop for MultilineAssignmentLayout {
    fn name(&self) -> &'static str {
        "Layout/MultilineAssignmentLayout"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        check_assignment::NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some((node, rhs)) = check_assignment::assignment(node) {
            self.check_assignment(cx, node, rhs);
        }
    }
}

impl MultilineAssignmentLayout {
    fn check_assignment<'a>(
        &mut self,
        cx: &mut Investigation<'a, '_>,
        node: Node<'a>,
        rhs: Option<Node<'a>>,
    ) {
        if node.is(NodeType::Send)
            && node.loc().operator.map(|op| node.range_source(op)) != Some("=")
        {
            return;
        }
        let Some(rhs) = rhs else {
            return;
        };
        if !self.supported_types(cx).contains(&rhs.ty()) {
            return;
        }
        let ps = cx.processed_source;
        if rhs.is_single_line()
            && (!rhs.is(NodeType::Block)
                || same_line(ps, node.source_range(), rhs.loc().begin.unwrap()))
        {
            return;
        }
        match style(cx) {
            "new_line" => check_new_line_offense(cx, node, rhs),
            "same_line" => check_same_line_offense(cx, node, rhs),
            _ => {}
        }
    }

    fn supported_types(&mut self, cx: &Investigation<'_, '_>) -> &[NodeType] {
        self.supported_types.get_or_insert_with(|| {
            cx.cop_config_list("SupportedTypes")
                .into_iter()
                .flat_map(|ty| {
                    if ty == "block" {
                        BLOCK_TYPES.to_vec()
                    } else {
                        NodeType::from_name(ty).into_iter().collect()
                    }
                })
                .collect()
        })
    }
}

fn check_new_line_offense(cx: &mut Investigation<'_, '_>, node: Node<'_>, rhs: Node<'_>) {
    let operator = node.loc().operator.unwrap();
    if !same_line(cx.processed_source, operator, rhs.source_range()) {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), NEW_LINE_OFFENSE, |corrector| {
        corrector.insert_after(operator, "\n");
    });
}

fn check_same_line_offense(cx: &mut Investigation<'_, '_>, node: Node<'_>, rhs: Node<'_>) {
    let operator = node.loc().operator.unwrap();
    if node.line_of(operator) == rhs.first_line() {
        return;
    }
    let rhs_begin = extract_rhs(node).unwrap().source_range().begin_pos;
    cx.add_offense_with_correction(node.source_range(), SAME_LINE_OFFENSE, |corrector| {
        corrector.replace(range_between(operator.end_pos, rhs_begin), " ");
    });
}
