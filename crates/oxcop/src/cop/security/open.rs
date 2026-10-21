//! Port of `lib/rubocop/cop/security/open.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static OPEN: NodePattern =
    NodePattern::new("(send ${nil? (const {nil? cbase} :URI)} :open $_ ...)");

#[derive(Default)]
pub struct Open;

impl Cop for Open {
    fn name(&self) -> &'static str {
        "Security/Open"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["open"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = OPEN.match_node(node) else {
            return;
        };
        let receiver = captures[0].node();
        let code = captures[1].node().unwrap();
        if safe(code) {
            return;
        }
        let receiver = match receiver {
            Some(r) => format!("{}.", r.source()),
            None => "Kernel#".to_owned(),
        };
        let message = format!("The use of `{receiver}open` is a serious security risk.");
        cx.add_offense(node.loc().selector.unwrap(), &message);
    }
}

fn safe(node: Node<'_>) -> bool {
    if node.is(NodeType::Str) {
        safe_argument(node.str_content().unwrap_or(""))
    } else if composite_string(node) {
        node.child(0).is_some_and(safe)
    } else {
        false
    }
}

fn safe_argument(argument: &str) -> bool {
    !argument.is_empty() && !argument.starts_with('|')
}

fn composite_string(node: Node<'_>) -> bool {
    node.is(NodeType::Dstr) || concatenated_string(node)
}

fn concatenated_string(node: Node<'_>) -> bool {
    node.is(NodeType::Send)
        && node.is_method("+")
        && node.receiver().is_some_and(|r| r.is(NodeType::Str))
}
