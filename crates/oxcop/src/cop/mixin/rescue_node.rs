//! Port of `lib/rubocop/cop/mixin/rescue_node.rb`.
//!
//! `modifier_locations` are the positions of `kRESCUE_MOD` tokens. They are
//! read from Prism's tree instead (the `keyword_loc` of every
//! `RescueModifierNode`), which is where the lexer's modifier state comes
//! from.

use ruby_prism::Visit;

use crate::ast::{Node, NodeType};
use crate::processed_source::{ProcessedSource, Range};

/// `modifier_locations`: the `rescue` keywords of modifier rescues.
pub fn modifier_locations(ps: &ProcessedSource<'_>) -> Vec<Range> {
    struct Finder {
        locations: Vec<Range>,
    }
    impl<'pr> Visit<'pr> for Finder {
        fn visit_rescue_modifier_node(&mut self, node: &ruby_prism::RescueModifierNode<'pr>) {
            let loc = node.keyword_loc();
            self.locations
                .push(Range::new(loc.start() as usize, loc.end() as usize));
            ruby_prism::visit_rescue_modifier_node(self, node);
        }
    }
    let mut finder = Finder {
        locations: Vec::new(),
    };
    finder.visit(&ps.prism_ast());
    finder.locations
}

/// `rescue_modifier?(node)`, given `modifier_locations`.
pub fn rescue_modifier(modifier_locations: &[Range], node: Node<'_>) -> bool {
    node.is(NodeType::Resbody)
        && node
            .loc()
            .keyword
            .is_some_and(|k| modifier_locations.contains(&k))
}
