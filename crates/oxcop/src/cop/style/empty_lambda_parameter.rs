//! Port of `lib/rubocop/cop/style/empty_lambda_parameter.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::empty_parameter::check;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Omit parentheses for the empty lambda parameters.";

#[derive(Default)]
pub struct EmptyLambdaParameter;

impl Cop for EmptyLambdaParameter {
    fn name(&self) -> &'static str {
        "Style/EmptyLambdaParameter"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let send_node = node.send_node().unwrap();
        if !send_node.is_send() || !send_node.is_lambda_literal() {
            return;
        }
        check(cx, node, MSG, |corrector, args| {
            let send_node = args.parent().unwrap().send_node().unwrap();
            let range = range_between(send_node.source_range().end_pos, args.source_range().end_pos);
            corrector.remove(range);
        });
    }
}
