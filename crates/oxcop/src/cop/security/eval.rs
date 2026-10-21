//! Port of `lib/rubocop/cop/security/eval.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "The use of `eval` is a serious security risk.";

static EVAL: NodePattern = NodePattern::new(
    "(send {nil? (send nil? :binding) (const {cbase nil?} :Kernel)} :eval $!str ...)",
);

#[derive(Default)]
pub struct Eval;

impl Cop for Eval {
    fn name(&self) -> &'static str {
        "Security/Eval"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["eval"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = EVAL.match_node(node) else {
            return;
        };
        let code = captures[0].node().unwrap();
        if code.is(NodeType::Dstr) && code.is_recursive_literal() {
            return;
        }
        cx.add_offense(node.loc().selector.unwrap(), MSG);
    }
}
