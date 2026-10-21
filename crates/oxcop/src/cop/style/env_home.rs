//! Port of `lib/rubocop/cop/style/env_home.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use `Dir.home` instead.";

static ENV_HOME: NodePattern = NodePattern::new(
    "(send
       (const {cbase nil?} :ENV) {:[] :fetch}
       (str \"HOME\")
       ...)",
);

#[derive(Default)]
pub struct EnvHome;

impl Cop for EnvHome {
    fn name(&self) -> &'static str {
        "Style/EnvHome"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["[]", "fetch"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !ENV_HOME.is_match(node) {
            return;
        }
        let arguments = node.arguments();
        if arguments.len() == 2 && !arguments[1].is(NodeType::Nil) {
            return;
        }
        if node.block_node().is_some() {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.replace(node.source_range(), "Dir.home");
        });
    }
}
