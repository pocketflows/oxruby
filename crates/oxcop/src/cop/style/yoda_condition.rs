//! Port of `lib/rubocop/cop/style/yoda_condition.rb`.

use crate::ast::node::COMPARISON_OPERATORS;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const ENFORCE_YODA_STYLES: &[&str] = &[
    "require_for_all_comparison_operators",
    "require_for_equality_operators_only",
];
const EQUALITY_ONLY_STYLES: &[&str] = &[
    "forbid_for_equality_operators_only",
    "require_for_equality_operators_only",
];

#[derive(Default)]
pub struct YodaCondition;

impl Cop for YodaCondition {
    fn name(&self) -> &'static str {
        "Style/YodaCondition"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        COMPARISON_OPERATORS
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_comparison_method() && !node.is_method("===")) {
            return;
        }
        let style = style(cx);
        let equality_only = EQUALITY_ONLY_STYLES.contains(&style);
        let non_equality_operator = !matches!(node.method_name(), Some("==" | "!="));
        if (equality_only && non_equality_operator)
            || file_constant_equal_program_name(node)
            || valid_yoda(style, node)
        {
            return;
        }
        let message = format!("Reverse the order of the operands `{}`.", node.source());
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), corrected_code(node));
        });
    }
}

/// `(send #source_file_path_constant? {:== :!=} (gvar #program_name?))`.
fn file_constant_equal_program_name(node: Node<'_>) -> bool {
    node.receiver().is_some_and(|r| r.source() == "__FILE__")
        && matches!(node.method_name(), Some("==" | "!="))
        && node.arguments().len() == 1
        && node.first_argument().is_some_and(|a| {
            a.is(NodeType::Gvar) && matches!(a.name(), Some("$0" | "$PROGRAM_NAME"))
        })
}

fn valid_yoda(style: &str, node: Node<'_>) -> bool {
    let Some(rhs) = node.first_argument() else {
        return true;
    };
    let Some(lhs) = node.receiver() else {
        return true;
    };
    if constant_portion(lhs) == constant_portion(rhs) || interpolation(lhs) {
        return true;
    }
    if ENFORCE_YODA_STYLES.contains(&style) {
        constant_portion(lhs)
    } else {
        constant_portion(rhs)
    }
}

fn corrected_code(node: Node<'_>) -> String {
    let lhs = node.receiver().unwrap();
    let rhs = node.first_argument().unwrap();
    format!(
        "{} {} {}",
        rhs.source(),
        reverse_comparison(node.method_name().unwrap()),
        lhs.source()
    )
}

fn constant_portion(node: Node<'_>) -> bool {
    node.is_recursive_literal() || node.is(NodeType::Const)
}

fn reverse_comparison(operator: &str) -> &str {
    match operator {
        "<" => ">",
        "<=" => ">=",
        ">" => "<",
        ">=" => "<=",
        other => other,
    }
}

fn interpolation(node: Node<'_>) -> bool {
    if node.is(NodeType::Dstr) {
        return true;
    }
    node.is(NodeType::Regexp) && node.child_nodes().any(|c| c.is(NodeType::Begin))
}
