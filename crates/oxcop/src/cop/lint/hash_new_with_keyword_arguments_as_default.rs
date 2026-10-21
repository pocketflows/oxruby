//! Port of `lib/rubocop/cop/lint/hash_new_with_keyword_arguments_as_default.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use a hash literal instead of keyword arguments.";

static HASH_NEW: NodePattern =
    NodePattern::new("(send (const {nil? (cbase)} :Hash) :new $[hash !braces?])");

#[derive(Default)]
pub struct HashNewWithKeywordArgumentsAsDefault;

impl Cop for HashNewWithKeywordArgumentsAsDefault {
    fn name(&self) -> &'static str {
        "Lint/HashNewWithKeywordArgumentsAsDefault"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(first_argument) = HASH_NEW.match_node(node).and_then(|c| c[0].node()) else {
            return;
        };
        let pairs = first_argument.pairs();
        if pairs.len() == 1
            && let Some(key) = pairs[0].key()
            && key.is(NodeType::Sym)
            && key.sym_value() == Some("capacity")
        {
            return;
        }
        cx.add_offense_with_correction(first_argument.source_range(), MSG, |corrector| {
            corrector.wrap(first_argument.source_range(), "{", "}");
        });
    }
}
