//! Port of `lib/rubocop/cop/style/empty_literal.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::frozen_string_literal::{
    frozen_string_literals_disabled, frozen_string_literals_enabled,
    string_literals_frozen_by_default,
};
use crate::cop::mixin::range_help::range_between;
use crate::cop::mixin::string_literals_help::preferred_string_literal;
use crate::cop::mixin::util::parentheses;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

static ARRAY_NODE: NodePattern =
    NodePattern::new("(send (const {nil? cbase} :Array) :new (array)?)");

static HASH_NODE: NodePattern = NodePattern::new("(send (const {nil? cbase} :Hash) :new)");

static STR_NODE: NodePattern = NodePattern::new("(send (const {nil? cbase} :String) :new)");

static ARRAY_WITH_BLOCK: NodePattern = NodePattern::new(
    "{
       (block (send (const {nil? cbase} :Array) :new) args _)
       ({numblock itblock} (send (const {nil? cbase} :Array) :new) ...)
     }",
);

static HASH_WITH_BLOCK: NodePattern = NodePattern::new(
    "{
       (block (send (const {nil? cbase} :Hash) :new) args _)
       ({numblock itblock} (send (const {nil? cbase} :Hash) :new) ...)
     }",
);

static ARRAY_WITH_INDEX: NodePattern = NodePattern::new(
    "{
       (send (const {nil? cbase} :Array) :[])
       (send nil? :Array (array))
     }",
);

static HASH_WITH_INDEX: NodePattern = NodePattern::new(
    "{
       (send (const {nil? cbase} :Hash) :[])
       (send nil? :Hash (array))
     }",
);

#[derive(Default)]
pub struct EmptyLiteral;

impl Cop for EmptyLiteral {
    fn name(&self) -> &'static str {
        "Style/EmptyLiteral"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new", "[]", "Array", "Hash"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(message) = offense_message(cx, node) else {
            return;
        };
        let range = replacement_range(node);
        let correction = correction(cx, node);
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(range, correction);
        });
    }
}

fn offense_message(cx: &Investigation<'_, '_>, node: Node<'_>) -> Option<String> {
    if offense_array_node(node) {
        Some(format!(
            "Use array literal `[]` instead of `{}`.",
            node.source()
        ))
    } else if offense_hash_node(node) {
        Some(format!(
            "Use hash literal `{{}}` instead of `{}`.",
            node.source()
        ))
    } else if STR_NODE.is_match(node) && !frozen_strings(cx) {
        Some(format!(
            "Use string literal `{}` instead of `String.new`.",
            preferred_string_literal(cx)
        ))
    } else {
        None
    }
}

fn first_argument_unparenthesized(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if !matches!(
        parent.ty(),
        NodeType::Send | NodeType::Super | NodeType::Zsuper
    ) {
        return false;
    }
    parent.first_argument() == Some(node) && !parentheses(parent)
}

fn replacement_range(node: Node<'_>) -> Range {
    if HASH_NODE.is_match(node) && first_argument_unparenthesized(node) {
        let args = node.parent().unwrap().arguments();
        range_between(
            args[0].source_range().begin_pos - 1,
            args[args.len() - 1].source_range().end_pos,
        )
    } else {
        node.source_range()
    }
}

fn offense_array_node(node: Node<'_>) -> bool {
    (ARRAY_NODE.is_match(node) && !node.parent().is_some_and(|p| ARRAY_WITH_BLOCK.is_match(p)))
        || ARRAY_WITH_INDEX.is_match(node)
}

fn offense_hash_node(node: Node<'_>) -> bool {
    (HASH_NODE.is_match(node) && !node.parent().is_some_and(|p| HASH_WITH_BLOCK.is_match(p)))
        || HASH_WITH_INDEX.is_match(node)
}

fn correction(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    if offense_array_node(node) {
        "[]".to_owned()
    } else if STR_NODE.is_match(node) {
        preferred_string_literal(cx).to_owned()
    } else if offense_hash_node(node) {
        if first_argument_unparenthesized(node) {
            let args = node.parent().unwrap().arguments();
            let mut parts = vec!["{}"];
            parts.extend(args[1..].iter().map(|a| a.source()));
            format!("({})", parts.join(", "))
        } else {
            "{}".to_owned()
        }
    } else {
        String::new()
    }
}

fn frozen_strings(cx: &Investigation<'_, '_>) -> bool {
    if frozen_string_literals_enabled(cx) {
        return true;
    }
    let frozen_string_cop_enabled = cx.config.cop_enabled("Style/FrozenStringLiteralComment");
    frozen_string_cop_enabled
        && !frozen_string_literals_disabled(cx.processed_source)
        && string_literals_frozen_by_default(cx).is_none()
}
