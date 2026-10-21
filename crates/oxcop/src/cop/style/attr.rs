//! Port of `lib/rubocop/cop/style/attr.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static CLASS_EVAL: NodePattern =
    NodePattern::new("(block (send _ {:class_eval :module_eval}) ...)");

#[derive(Default)]
pub struct Attr;

impl Cop for Attr {
    fn name(&self) -> &'static str {
        "Style/Attr"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["attr"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_command("attr") || !node.has_arguments() {
            return;
        }
        if allowed_context(node) {
            return;
        }
        let replacement = replacement_method(node);
        let message = format!("Do not use `attr`. Use `{replacement}` instead.");
        cx.add_offense_with_correction(node.loc().selector.unwrap(), &message, |corrector| {
            let arguments = node.arguments();
            let attr_name = arguments[0];
            let setter = arguments.get(1);
            corrector.replace(node.loc().selector.unwrap(), replacement);
            if setter.is_some_and(|s| s.is_boolean()) {
                corrector.remove(range_between(
                    attr_name.source_range().end_pos,
                    node.source_range().end_pos,
                ));
            }
        });
    }
}

fn allowed_context(node: Node<'_>) -> bool {
    let Some(class_node) = node
        .ancestors()
        .find(|a| matches!(a.ty(), NodeType::Class | NodeType::Block))
    else {
        return false;
    };
    (!class_node.is(NodeType::Class) && !CLASS_EVAL.is_match(class_node))
        || define_attr_method(class_node)
}

fn define_attr_method(node: Node<'_>) -> bool {
    node.descendants()
        .any(|d| d.is(NodeType::Def) && d.is_method("attr"))
}

fn replacement_method(node: Node<'_>) -> &'static str {
    match node.last_argument() {
        Some(setter) if setter.is(NodeType::True) => "attr_accessor",
        _ => "attr_reader",
    }
}
