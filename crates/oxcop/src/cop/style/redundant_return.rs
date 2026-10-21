//! Port of `lib/rubocop/cop/style/redundant_return.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::{Side, SurroundingSpace, range_with_surrounding_space};
use crate::cop::{Corrector, Cop, Investigation};

const MSG: &str = "Redundant `return` detected.";
const MULTI_RETURN_MSG: &str = "To return multiple values, use an array.";

#[derive(Default)]
pub struct RedundantReturn;

impl Cop for RedundantReturn {
    fn name(&self) -> &'static str {
        "Style/RedundantReturn"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Def, NodeType::Defs]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["define_method", "define_singleton_method", "lambda"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_send() {
            if !node.is_block_literal() {
                return;
            }
            check_branch(cx, node.parent().unwrap().body());
        } else {
            check_branch(cx, node.body());
        }
    }
}

fn check_branch<'a>(cx: &mut Investigation<'a, '_>, node: Option<Node<'a>>) {
    let Some(node) = node else {
        return;
    };
    match node.ty() {
        NodeType::Return => check_return_node(cx, node),
        NodeType::Case | NodeType::CaseMatch => {
            for when_node in node.when_branches() {
                check_branch(cx, when_node.body());
            }
            check_branch(cx, node.else_branch());
        }
        NodeType::If => {
            if node.is_ternary() {
                return;
            }
            check_branch(cx, node.if_branch());
            check_branch(cx, node.else_branch());
        }
        NodeType::Rescue => {
            for branch in node.branches() {
                check_branch(cx, branch);
            }
            if !node.has_else() {
                check_branch(cx, node.body());
            }
        }
        NodeType::Resbody => check_branch(cx, node.body()),
        NodeType::Ensure => check_branch(cx, node.child(0)),
        NodeType::Begin | NodeType::Kwbegin => check_branch(cx, node.child_nodes().last()),
        _ => {}
    }
}

fn check_return_node<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let allow_multiple = cx.cop_config_bool("AllowMultipleReturnValues", false);
    let multiple = node.children().len() > 1;
    if allow_multiple && multiple {
        return;
    }
    let message = if !allow_multiple && multiple {
        format!("{MSG} {MULTI_RETURN_MSG}")
    } else {
        MSG.to_owned()
    };
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(node.loc().keyword.unwrap(), &message, |corrector| {
        if node.has_arguments() {
            correct_with_arguments(src, node, corrector);
        } else {
            corrector.replace(node.source_range(), "nil");
        }
    });
}

fn correct_with_arguments(src: &str, return_node: Node<'_>, corrector: &mut Corrector) {
    let first_argument = return_node.first_argument().unwrap();
    if return_node.children().len() > 1 {
        add_brackets(corrector, return_node);
    } else if first_argument.is(NodeType::Hash) && !first_argument.has_braces() {
        add_braces(corrector, first_argument);
    }
    if return_node.is_splat_argument() {
        corrector.replace(
            first_argument.source_range(),
            first_argument.source().strip_prefix('*').unwrap_or(first_argument.source()),
        );
    }
    let keyword = range_with_surrounding_space(
        src,
        return_node.loc().keyword.unwrap(),
        SurroundingSpace::side(Side::Right),
    );
    corrector.remove(keyword);
}

fn add_brackets(corrector: &mut Corrector, node: Node<'_>) {
    corrector.insert_before(node.child_nodes().next().unwrap().source_range(), "[");
    corrector.insert_after(node.child_nodes().last().unwrap().source_range(), "]");
}

fn add_braces(corrector: &mut Corrector, node: Node<'_>) {
    corrector.insert_before(node.child_nodes().next().unwrap().source_range(), "{");
    corrector.insert_after(node.child_nodes().last().unwrap().source_range(), "}");
}
