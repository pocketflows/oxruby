//! Port of `lib/rubocop/cop/style/safe_navigation_chain_length.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SafeNavigationChainLength;

impl Cop for SafeNavigationChainLength {
    fn name(&self) -> &'static str {
        "Style/SafeNavigationChainLength"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let chains: Vec<Node<'a>> = node.ancestors().take_while(|p| p.is_csend()).collect();
        let max = cx.cop_config_int("Max").unwrap_or(2);
        if (chains.len() as i64) < max {
            return;
        }
        let Some(last) = chains.last() else {
            return;
        };
        let message = format!("Avoid safe navigation chains longer than {max} calls.");
        cx.add_offense(last.source_range(), &message);
    }
}
