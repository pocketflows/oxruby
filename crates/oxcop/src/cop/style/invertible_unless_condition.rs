//! Port of `lib/rubocop/cop/style/invertible_unless_condition.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};

#[derive(Default)]
pub struct InvertibleUnlessCondition;

impl Cop for InvertibleUnlessCondition {
    fn name(&self) -> &'static str {
        "Style/InvertibleUnlessCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_unless() {
            return;
        }
        let Some(condition) = node.condition() else {
            return;
        };
        let inverse = InverseMethods(cx.cop_config_value("InverseMethods"));
        if !invertible(&inverse, Some(condition)) {
            return;
        }
        let message = format!(
            "Prefer `{} {}` over `{} {}`.",
            node.inverse_keyword(),
            preferred_condition(&inverse, condition),
            node.keyword(),
            condition.source()
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.loc().keyword.unwrap(), node.inverse_keyword());
            autocorrect(&inverse, corrector, condition);
        });
    }
}

/// `cop_config['InverseMethods']`: symbol keys (`:!=`) to symbol values.
struct InverseMethods<'a>(Option<&'a Value>);

impl InverseMethods<'_> {
    fn get(&self, method: &str) -> Option<&str> {
        let Some(Value::Mapping(map)) = self.0 else {
            return None;
        };
        map.iter().find_map(|(k, v)| {
            let key = k.as_str()?.trim_start_matches(':');
            (key == method).then(|| v.as_str().map(|s| s.trim_start_matches(':')))?
        })
    }
}

fn invertible(inverse: &InverseMethods<'_>, node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    match node.ty() {
        NodeType::Begin => node.children().len() == 1 && invertible(inverse, node.child(0)),
        NodeType::Send => {
            if inheritance_check(node) {
                return false;
            }
            node.is_method("!") || node.method_name().is_some_and(|m| inverse.get(m).is_some())
        }
        NodeType::Or | NodeType::And => invertible(inverse, node.lhs()) && invertible(inverse, node.rhs()),
        _ => false,
    }
}

fn inheritance_check(node: Node<'_>) -> bool {
    let Some(argument) = node.first_argument() else {
        return false;
    };
    node.is_method("<")
        && argument.is(NodeType::Const)
        && argument
            .short_name()
            .is_some_and(|n| n.to_uppercase() != n)
}

fn preferred_condition(inverse: &InverseMethods<'_>, node: Node<'_>) -> String {
    match node.ty() {
        NodeType::Begin => format!("({})", preferred_condition(inverse, node.child(0).unwrap())),
        NodeType::Send => preferred_send_condition(inverse, node),
        _ => preferred_logical_condition(inverse, node),
    }
}

fn preferred_send_condition(inverse: &InverseMethods<'_>, node: Node<'_>) -> String {
    let receiver_source = node.receiver().map(|r| r.source());
    if node.is_method("!") {
        return receiver_source.unwrap_or("").to_owned();
    }
    let dotted = receiver_source.map_or(String::new(), |r| format!("{r}."));
    let inverse_name = inverse.get(node.method_name().unwrap()).unwrap_or("");
    if !node.has_arguments() {
        return format!("{dotted}{inverse_name}");
    }
    let args: Vec<&str> = node.arguments().iter().map(|a| a.source()).collect();
    let argument_list = args.join(", ");
    if node.is_operator_method() {
        return format!("{} {inverse_name} {argument_list}", receiver_source.unwrap_or(""));
    }
    if node.is_parenthesized() {
        return format!("{dotted}{inverse_name}({argument_list})");
    }
    format!("{dotted}{inverse_name} {argument_list}")
}

fn inverse_operator(node: Node<'_>) -> &'static str {
    let logical = node.is_logical_operator();
    match (node.ty(), logical) {
        (NodeType::And, true) => "||",
        (NodeType::And, false) => "or",
        (_, true) => "&&",
        (_, false) => "and",
    }
}

fn preferred_logical_condition(inverse: &InverseMethods<'_>, node: Node<'_>) -> String {
    let lhs = preferred_operand(inverse, node, node.lhs().unwrap());
    let rhs = preferred_operand(inverse, node, node.rhs().unwrap());
    format!("{lhs} {} {rhs}", inverse_operator(node))
}

fn preferred_operand(inverse: &InverseMethods<'_>, node: Node<'_>, operand: Node<'_>) -> String {
    let preferred = preferred_condition(inverse, operand);
    if parenthesize_inverted_operand(node, operand) {
        format!("({preferred})")
    } else {
        preferred
    }
}

fn autocorrect(inverse: &InverseMethods<'_>, corrector: &mut Corrector, node: Node<'_>) {
    match node.ty() {
        NodeType::Begin => autocorrect(inverse, corrector, node.child(0).unwrap()),
        NodeType::Send => {
            if node.is_method("!") {
                corrector.remove(node.loc().selector.unwrap());
            } else {
                corrector.replace(
                    node.loc().selector.unwrap(),
                    inverse.get(node.method_name().unwrap()).unwrap_or(""),
                );
            }
        }
        NodeType::Or | NodeType::And => {
            corrector.replace(node.loc().operator.unwrap(), inverse_operator(node));
            for operand in [node.lhs().unwrap(), node.rhs().unwrap()] {
                autocorrect(inverse, corrector, operand);
                if parenthesize_inverted_operand(node, operand) {
                    corrector.wrap(operand.source_range(), "(", ")");
                }
            }
        }
        _ => {}
    }
}

fn parenthesize_inverted_operand(node: Node<'_>, operand: Node<'_>) -> bool {
    node.is(NodeType::Or) && operand.is(NodeType::And)
}
