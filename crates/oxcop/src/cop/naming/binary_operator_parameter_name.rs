//! Port of `lib/rubocop/cop/naming/binary_operator_parameter_name.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::regex;

const OP_LIKE_METHODS: &[&str] = &["eql?", "equal?"];
const EXCLUDED: &[&str] = &["+@", "-@", "[]", "[]=", "<<", "===", "`", "=~"];

#[derive(Default)]
pub struct BinaryOperatorParameterName;

impl Cop for BinaryOperatorParameterName {
    fn name(&self) -> &'static str {
        "Naming/BinaryOperatorParameterName"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // (def [#op_method? $_] (args $(arg [!:other !:_other])) _)
        let Some(name) = node.method_name() else {
            return;
        };
        if !op_method(name) {
            return;
        }
        let Some(args) = node.arguments_node() else {
            return;
        };
        let [arg] = args.children() else {
            return;
        };
        let Some(arg) = arg.as_node().map(|id| node.ast().node(id)) else {
            return;
        };
        if !arg.is(NodeType::Arg) || matches!(arg.name(), Some("other" | "_other")) {
            return;
        }
        let message = format!("When defining the `{name}` operator, name its argument `other`.");
        cx.add_offense_with_correction(arg.source_range(), &message, |corrector| {
            corrector.replace(arg.source_range(), "other");
            for lvar in node
                .descendants()
                .filter(|d| matches!(d.ty(), NodeType::Lvar | NodeType::Lvasgn))
            {
                let Some(lvar_location) = lvar.loc().name else {
                    continue;
                };
                if lvar.range_source(lvar_location) != arg.source() {
                    continue;
                }
                if shadowed(lvar, arg.source(), node) {
                    continue;
                }
                corrector.replace(lvar_location, "other");
            }
        });
    }
}

fn op_method(name: &str) -> bool {
    if EXCLUDED.contains(&name) {
        return false;
    }
    regex!(r"\A[[:word:]]").find(name).is_none() || OP_LIKE_METHODS.contains(&name)
}

/// A reference is shadowed when an enclosing block within the method
/// redeclares the parameter name (as a block parameter or block-local).
fn shadowed(node: Node<'_>, name: &str, def_node: Node<'_>) -> bool {
    node.ancestors()
        .filter(|a| a.is(NodeType::Block))
        .any(|block| {
            def_node.source_range().contains(block.source_range()) && redeclares(block, name)
        })
}

fn redeclares(block: Node<'_>, name: &str) -> bool {
    block
        .arguments()
        .iter()
        .any(|argument| argument.name().is_some_and(|n| n == name))
}
