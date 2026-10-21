//! Port of `lib/rubocop/cop/style/array_intersect.rb`.

use crate::ast::node_pattern::{Captures, MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const PREDICATES: &[&str] = &["any?", "empty?", "none?"];
const ACTIVE_SUPPORT_PREDICATES: &[&str] = &["any?", "empty?", "none?", "present?", "blank?"];
const ARRAY_SIZE_METHODS: &[&str] = &["count", "length", "size"];
const STRAIGHT_METHODS: &[&str] = &["present?", "any?", ">", "positive?", "!="];

static BAD_INTERSECTION_CHECK: NodePattern = NodePattern::new(
    "$(call
       {
         (begin (send $_ :& $_))
         (call $!nil? :intersection $_)
       }
       $%1
     )",
);

static INTERSECTION_SIZE_CHECK: NodePattern = NodePattern::new(
    "(call
       $(call
         {
           (begin (send $_ :& $_))
           (call $!nil? :intersection $_)
         }
         %ARRAY_SIZE_METHODS
       )
       {$:> (int 0) | $:positive? | $:!= (int 0) | $:== (int 0) | $:zero?}
     )",
);

static ANY_NONE_BLOCK_INTERSECTION: NodePattern = NodePattern::new(
    "{
       (block
         (call $_receiver ${:any? :none?})
         (args (arg _key))
         (send $!nil? ${:member? :include?} (lvar _key))
       )
       (numblock
         (call $_receiver ${:any? :none?}) 1
         (send $!nil? ${:member? :include?} (lvar :_1))
       )
       (itblock
         (call $_receiver ${:any? :none?}) :it
         (send $!nil? ${:member? :include?} (lvar :it))
       )
     }",
);

#[derive(Default)]
pub struct ArrayIntersect;

impl Cop for ArrayIntersect {
    fn name(&self) -> &'static str {
        "Style/ArrayIntersect"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.1)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Send,
            NodeType::Csend,
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
        ]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "present?",
            "any?",
            ">",
            "positive?",
            "!=",
            "blank?",
            "empty?",
            "none?",
            "==",
            "zero?",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_block() {
            on_block(cx, node);
            return;
        }
        if node.is_block_literal() {
            return;
        }
        let Some(captures) = bad_intersection(cx, node) else {
            return;
        };
        let (Some(dot_node), Some(receiver), Some(argument), Some(method_name)) = (
            captures[0].node(),
            captures[1].node(),
            captures[2].node(),
            captures[3].sym(),
        ) else {
            return;
        };
        let Some(dot) = dot_node.loc().dot.map(|d| dot_node.range_source(d)) else {
            return;
        };
        let bang = if STRAIGHT_METHODS.contains(&method_name) {
            ""
        } else {
            "!"
        };
        if bang == "!" && dot == "&." {
            return;
        }
        let replacement = format!(
            "{bang}{}{dot}intersect?({})",
            receiver.source(),
            argument.source()
        );
        register_offense(cx, node, replacement);
    }
}

fn on_block<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    let Some(captures) = ANY_NONE_BLOCK_INTERSECTION.match_node(node) else {
        return;
    };
    let (Some(receiver), Some(method_name), Some(argument), Some(block_method)) = (
        captures[0].node(),
        captures[1].sym(),
        captures[2].node(),
        captures[3].sym(),
    ) else {
        return;
    };
    let send_node = node.send_node().unwrap();
    if method_name == "none?" && send_node.is_safe_navigation() {
        return;
    }
    if block_method == "include?" && !argument.is(NodeType::Array) {
        return;
    }
    let Some(dot) = send_node.loc().dot.map(|d| node.range_source(d)) else {
        return;
    };
    let bang = if method_name == "any?" { "" } else { "!" };
    let replacement = format!(
        "{bang}{}{dot}intersect?({})",
        receiver.source(),
        argument.source()
    );
    register_offense(cx, node, replacement);
}

fn bad_intersection<'a>(cx: &Investigation<'_, '_>, node: Node<'a>) -> Option<Captures<'a>> {
    let predicates = if cx
        .config
        .all_cops_bool("ActiveSupportExtensionsEnabled", false)
    {
        ACTIVE_SUPPORT_PREDICATES
    } else {
        PREDICATES
    };
    let params = [Param::Syms(predicates)];
    let ctx = MatchContext {
        params: &params,
        ..Default::default()
    };
    if let Some(captures) = BAD_INTERSECTION_CHECK.match_with(node, &ctx) {
        return Some(captures);
    }
    let named = [("ARRAY_SIZE_METHODS", Param::Syms(ARRAY_SIZE_METHODS))];
    let ctx = MatchContext {
        named: &named,
        ..Default::default()
    };
    INTERSECTION_SIZE_CHECK.match_with(node, &ctx)
}

fn register_offense(cx: &mut Investigation<'_, '_>, node: Node<'_>, replacement: String) {
    let message = format!(
        "Use `{replacement}` instead of `{}`.",
        node.source()
    );
    cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
        corrector.replace(node.source_range(), replacement);
    });
}
