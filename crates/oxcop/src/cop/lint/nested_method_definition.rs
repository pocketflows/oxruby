//! Port of `lib/rubocop/cop/lint/nested_method_definition.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Method definitions must not be nested. Use `lambda` instead.";

static EVAL_CALL: NodePattern =
    NodePattern::new("(any_block (send _ {:instance_eval :class_eval :module_eval} ...) ...)");
static EXEC_CALL: NodePattern =
    NodePattern::new("(any_block (send _ {:instance_exec :class_exec :module_exec} ...) ...)");

#[derive(Default)]
pub struct NestedMethodDefinition;

impl Cop for NestedMethodDefinition {
    fn name(&self) -> &'static str {
        "Lint/NestedMethodDefinition"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Defs) && node.child(0).is_some_and(allowed_subject_type) {
            return;
        }
        let def_ancestor = node.ancestors().find(|a| a.is_any_def());
        if def_ancestor.is_none() {
            return;
        }
        let within_scoping_def = node
            .ancestors()
            .filter(|a| a.is_any_block() || a.is(NodeType::Sclass))
            .any(|ancestor| scoping_method_call(cx, ancestor));
        if !within_scoping_def {
            cx.add_offense(node.source_range(), MSG);
        }
    }
}

fn scoping_method_call(cx: &Investigation<'_, '_>, child: Node<'_>) -> bool {
    child.is(NodeType::Sclass)
        || EVAL_CALL.is_match(child)
        || EXEC_CALL.is_match(child)
        || child.is_class_constructor()
        || allowed_method_name(cx, child)
}

fn allowed_subject_type(subject: Node<'_>) -> bool {
    subject.is_variable() || subject.is(NodeType::Const) || subject.is_call()
}

fn allowed_method_name(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let name = node.method_name().unwrap_or_default();
    allowed_method(cx, name) || matches_allowed_pattern(cx, name)
}
