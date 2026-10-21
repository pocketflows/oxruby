//! Port of `lib/rubocop/cop/style/empty_class_definition.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MSG_CLASS_KEYWORD: &str =
    "Use the `class` keyword instead of `Class.new` to define an empty class.";
const MSG_CLASS_NEW: &str =
    "Use `Class.new` instead of the `class` keyword to define an empty class.";

static CLASS_NEW_ASSIGNMENT: NodePattern =
    NodePattern::new("(casgn _ _ $(send (const _ :Class) :new _))");

#[derive(Default)]
pub struct EmptyClassDefinition;

impl Cop for EmptyClassDefinition {
    fn name(&self) -> &'static str {
        "Style/EmptyClassDefinition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Casgn, NodeType::Class]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        let allowed = cx.cop_config_list("AllowedParentClasses");
        if node.is(NodeType::Casgn) {
            if !matches!(style, "class_keyword" | "class_definition") {
                return;
            }
            let Some(class_new_node) = CLASS_NEW_ASSIGNMENT
                .match_node(node)
                .and_then(|c| c[0].node())
            else {
                return;
            };
            let Some(arg) = class_new_node.first_argument() else {
                return;
            };
            if !arg.is(NodeType::Const) || allowed.contains(&arg.source()) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), MSG_CLASS_KEYWORD, |corrector| {
                let indent = " ".repeat(node.column());
                let name = node.name().unwrap_or("");
                let class_name = match node.namespace() {
                    Some(namespace) => format!("{}::{name}", namespace.source()),
                    None => name.to_owned(),
                };
                corrector.replace(
                    node.source_range(),
                    format!("class {class_name} < {}\n{indent}end", arg.source()),
                );
            });
        } else {
            if style != "class_new" {
                return;
            }
            let Some(parent_class) = node.parent_class() else {
                return;
            };
            if node.body().is_some_and(|b| !b.children().is_empty()) {
                return;
            }
            if allowed.contains(&parent_class.source()) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), MSG_CLASS_NEW, |corrector| {
                corrector.replace(
                    node.source_range(),
                    format!(
                        "{} = Class.new({})",
                        node.identifier().unwrap().source(),
                        parent_class.source()
                    ),
                );
            });
        }
    }
}
