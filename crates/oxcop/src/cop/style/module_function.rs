//! Port of `lib/rubocop/cop/style/module_function.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MODULE_FUNCTION_MSG: &str = "Use `module_function` instead of `extend self`.";
const EXTEND_SELF_MSG: &str = "Use `extend self` instead of `module_function`.";
const FORBIDDEN_MSG: &str = "Do not use `module_function` or `extend self`.";

static MODULE_FUNCTION_NODE: NodePattern = NodePattern::new("(send nil? :module_function)");
static EXTEND_SELF_NODE: NodePattern = NodePattern::new("(send nil? :extend self)");
static PRIVATE_DIRECTIVE: NodePattern = NodePattern::new("(send nil? :private ...)");

#[derive(Default)]
pub struct ModuleFunction;

impl Cop for ModuleFunction {
    fn name(&self) -> &'static str {
        "Style/ModuleFunction"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = node.body() else {
            return;
        };
        let body_nodes: Vec<Node<'a>> = if body.is(NodeType::Begin) {
            body.child_nodes().collect()
        } else {
            vec![body]
        };
        let style = style(cx);
        let mut wrong = Vec::new();
        match style {
            "module_function" => {
                if !body_nodes.iter().any(|n| PRIVATE_DIRECTIVE.is_match(*n)) {
                    wrong.extend(body_nodes.iter().filter(|n| EXTEND_SELF_NODE.is_match(**n)));
                }
            }
            "extend_self" => {
                wrong.extend(body_nodes.iter().filter(|n| MODULE_FUNCTION_NODE.is_match(**n)));
            }
            "forbidden" => {
                for n in &body_nodes {
                    if EXTEND_SELF_NODE.is_match(*n) {
                        wrong.push(n);
                    }
                    if MODULE_FUNCTION_NODE.is_match(*n) {
                        wrong.push(n);
                    }
                }
            }
            _ => {}
        }
        let message = match style {
            "forbidden" => FORBIDDEN_MSG,
            "module_function" => MODULE_FUNCTION_MSG,
            _ => EXTEND_SELF_MSG,
        };
        for child_node in wrong {
            let child_node = *child_node;
            cx.add_offense_with_correction(child_node.source_range(), message, |corrector| {
                if style == "forbidden" {
                    return;
                }
                if EXTEND_SELF_NODE.is_match(child_node) {
                    corrector.replace(child_node.source_range(), "module_function");
                } else {
                    corrector.replace(child_node.source_range(), "extend self");
                }
            });
        }
    }
}
