//! Port of `lib/rubocop/cop/style/character_literal.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::string_help::{self, StringHelp};
use crate::cop::{Corrector, Cop, Investigation};

const MSG: &str = "Do not use the character literal - use string literal instead.";

#[derive(Default)]
pub struct CharacterLiteral {
    string_help: StringHelp,
}

impl Cop for CharacterLiteral {
    fn name(&self) -> &'static str {
        "Style/CharacterLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        string_help::NODE_TYPES
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(node) = self.string_help.candidate(node) else {
            return;
        };
        if !offense(node) {
            return;
        }
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            autocorrect(corrector, node);
        });
    }
}

fn offense(node: Node<'_>) -> bool {
    node.is_character_literal() && (2..=3).contains(&node.source().chars().count())
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    let string: String = node.source().chars().skip(1).collect();
    let length = string.chars().count();
    if length == 2 || string == "'" {
        corrector.replace(node.source_range(), format!("\"{string}\""));
    } else if length == 1 {
        corrector.replace(node.source_range(), format!("'{string}'"));
    }
}
