//! Port of `lib/rubocop/cop/lint/unified_integer.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static FIXNUM_OR_BIGNUM_CONST: NodePattern =
    NodePattern::new("(:const {nil? (:cbase)} ${:Fixnum :Bignum})");

#[derive(Default)]
pub struct UnifiedInteger;

impl Cop for UnifiedInteger {
    fn name(&self) -> &'static str {
        "Lint/UnifiedInteger"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Const]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(klass) = FIXNUM_OR_BIGNUM_CONST
            .match_node(node)
            .and_then(|c| c[0].sym())
        else {
            return;
        };

        let message = format!("Use `Integer` instead of `{klass}`.");
        let correct = cx.target_ruby_version() > 2.3;
        let name = node.loc().name;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if !correct {
                return;
            }
            if let Some(name) = name {
                corrector.replace(name, "Integer");
            }
        });
    }
}
