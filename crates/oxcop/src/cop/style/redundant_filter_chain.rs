//! Port of `lib/rubocop/cop/style/redundant_filter_chain.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const RAILS_METHODS: &[&str] = &["many?", "present?"];

static SELECT_PREDICATE: NodePattern = NodePattern::new(
    "(call
      {
        (any_block $(call _ {:select :filter :find_all}) ...)
        $(call _ {:select :filter :find_all} block_pass_type?)
      }
      ${:any? :empty? :none? :one? :many? :present?})",
);

fn replacement_method(name: &str) -> &'static str {
    match name {
        "any?" | "present?" => "any?",
        "empty?" | "none?" => "none?",
        "one?" => "one?",
        "many?" => "many?",
        _ => "",
    }
}

#[derive(Default)]
pub struct RedundantFilterChain;

impl Cop for RedundantFilterChain {
    fn name(&self) -> &'static str {
        "Style/RedundantFilterChain"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["any?", "empty?", "none?", "one?", "many?", "present?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.has_arguments() || node.is_block_literal() {
            return;
        }
        let Some(c) = SELECT_PREDICATE.match_node(node) else {
            return;
        };
        let select_node = c[0].node().unwrap();
        let filter_method = c[1].sym().unwrap_or("");
        if RAILS_METHODS.contains(&filter_method)
            && !cx.config.all_cops_bool("ActiveSupportExtensionsEnabled", false)
        {
            return;
        }
        let predicate_name = node.method_name().unwrap_or("");
        let replacement = replacement_method(predicate_name);
        let message = format!(
            "Use `{replacement}` instead of `{}.{predicate_name}`.",
            select_node.method_name().unwrap_or("")
        );
        let select_selector = select_node.loc().selector.unwrap();
        let offense_range = select_selector.join(node.loc().selector.unwrap());
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            let predicate_range = node
                .receiver()
                .unwrap()
                .source_range()
                .end()
                .join(node.loc().selector.unwrap());
            corrector.remove(predicate_range);
            corrector.replace(select_selector, replacement);
        });
    }
}
