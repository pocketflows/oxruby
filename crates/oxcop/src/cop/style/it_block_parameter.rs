//! Port of `lib/rubocop/cop/style/it_block_parameter.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MSG_USE_IT_PARAMETER: &str = "Use `it` block parameter.";
const MSG_AVOID_IT_PARAMETER: &str = "Avoid using `it` block parameter.";
const MSG_AVOID_IT_PARAMETER_MULTILINE: &str =
    "Avoid using `it` block parameter for multi-line blocks.";

#[derive(Default)]
pub struct ItBlockParameter;

impl Cop for ItBlockParameter {
    fn name(&self) -> &'static str {
        "Style/ItBlockParameter"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.4)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let style = style(cx);
        match node.ty() {
            NodeType::Block => {
                if style != "always" {
                    return;
                }
                let arguments = node.arguments();
                if arguments.len() != 1 {
                    return;
                }
                // `restarg`, `kwrestarg`, `blockarg` nodes can return early.
                let first = arguments[0];
                if !first.is(NodeType::Arg) {
                    return;
                }
                let args = node.child(1).unwrap();
                for variable in find_block_variables(node, first.source()) {
                    cx.add_offense_with_correction(
                        variable.source_range(),
                        MSG_USE_IT_PARAMETER,
                        |corrector| {
                            corrector.remove(args.source_range());
                            corrector.replace(variable.source_range(), "it");
                        },
                    );
                }
            }
            NodeType::Numblock => {
                if style == "disallow" || node.children().get(1) != Some(&Child::Int(1)) {
                    return;
                }
                for variable in find_block_variables(node, "_1") {
                    cx.add_offense_with_correction(
                        variable.source_range(),
                        MSG_USE_IT_PARAMETER,
                        |corrector| {
                            corrector.replace(variable.source_range(), "it");
                        },
                    );
                }
            }
            _ => match style {
                "allow_single_line" => {
                    if !node.is_single_line() {
                        cx.add_offense(node.source_range(), MSG_AVOID_IT_PARAMETER_MULTILINE);
                    }
                }
                "disallow" => {
                    for variable in find_block_variables(node, "it") {
                        cx.add_offense(variable.source_range(), MSG_AVOID_IT_PARAMETER);
                    }
                }
                _ => {}
            },
        }
    }
}

fn find_block_variables<'a>(node: Node<'a>, block_argument_name: &str) -> Vec<Node<'a>> {
    let Some(body) = node.body() else {
        return Vec::new();
    };
    body.descendants()
        .filter(|d| d.is(NodeType::Lvar) && d.source() == block_argument_name)
        .collect()
}
