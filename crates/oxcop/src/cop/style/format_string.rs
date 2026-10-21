//! Port of `lib/rubocop/cop/style/format_string.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Corrector, Investigation};

/// Known conversion methods whose return value is not an array.
const AUTOCORRECTABLE_METHODS: &[&str] = &["to_d", "to_f", "to_h", "to_i", "to_r", "to_s", "to_sym"];

static FORMATTER: NodePattern = NodePattern::new(
    "{
      (send nil? ${:sprintf :format} _ _ ...)
      (send {str dstr} $:% ... )
      (send !nil? $:% {array hash})
    }",
);
static VARIABLE_ARGUMENT: NodePattern = NodePattern::new("(send {str dstr} :% #autocorrectable?)");

#[derive(Default)]
pub struct FormatString;

impl Cop for FormatString {
    fn name(&self) -> &'static str {
        "Style/FormatString"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["format", "sprintf", "%"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(selector) = FORMATTER.match_node(node).and_then(|c| c[0].sym()) else {
            return;
        };
        let detected_style = if selector == "%" { "percent" } else { selector };
        let style = style(cx);
        if detected_style == style {
            return;
        }
        let message = format!(
            "Favor `{}` over `{}`.",
            method_name(style),
            method_name(detected_style)
        );
        cx.add_offense_with_correction(node.loc().selector.unwrap(), &message, |corrector| {
            autocorrect(style, corrector, node);
        });
    }
}

fn autocorrectable(node: Node<'_>) -> bool {
    if node.is(NodeType::Lvar) {
        return true;
    }
    node.is(NodeType::Send)
        && !node
            .method_name()
            .is_some_and(|m| AUTOCORRECTABLE_METHODS.contains(&m))
}

fn variable_argument(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "autocorrectable?" && val.node().is_some_and(autocorrectable)
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    VARIABLE_ARGUMENT.match_with(node, &ctx).is_some()
}

fn method_name(style_name: &str) -> &str {
    if style_name == "percent" {
        "String#%"
    } else {
        style_name
    }
}

fn autocorrect(style: &str, corrector: &mut Corrector, node: Node<'_>) {
    if variable_argument(node) {
        return;
    }
    match node.method_name() {
        Some("%") => autocorrect_from_percent(style, corrector, node),
        Some("format" | "sprintf") => match style {
            "percent" => autocorrect_to_percent(corrector, node),
            "format" | "sprintf" => corrector.replace(node.loc().selector.unwrap(), style),
            _ => {}
        },
        _ => {}
    }
}

fn autocorrect_from_percent(style: &str, corrector: &mut Corrector, node: Node<'_>) {
    let percent_rhs = node.first_argument().unwrap();
    let args = match percent_rhs.ty() {
        NodeType::Array | NodeType::Hash => percent_rhs
            .child_nodes()
            .map(|c| c.source())
            .collect::<Vec<_>>()
            .join(", "),
        _ => percent_rhs.source().to_owned(),
    };
    let corrected = format!("{style}({}, {args})", node.receiver().unwrap().source());
    corrector.replace(node.source_range(), corrected);
}

fn autocorrect_to_percent(corrector: &mut Corrector, node: Node<'_>) {
    let arguments = node.arguments();
    let (format_arg, param_args) = arguments.split_first().unwrap();
    let format = format_arg.source();
    let args = if param_args.len() == 1 {
        format_single_parameter(*param_args.last().unwrap())
    } else {
        let sources: Vec<&str> = param_args.iter().map(|a| a.source()).collect();
        format!("[{}]", sources.join(", "))
    };
    corrector.replace(node.source_range(), format!("{format} % {args}"));
}

fn format_single_parameter(arg: Node<'_>) -> String {
    // `format(fmt, *args)` is equivalent to `fmt % args`, so unwrap the splat
    // and render the argument it splats.
    if arg.is(NodeType::Splat) {
        return format_single_parameter(arg.child(0).unwrap());
    }
    let source = arg.source();
    if arg.is(NodeType::Hash) {
        return format!("{{ {source} }}");
    }
    if requires_parentheses(arg) {
        format!("({source})")
    } else {
        source.to_owned()
    }
}

/// An argument that binds looser than `%` (a ternary, range, assignment, or
/// operator call) must be parenthesized to keep its meaning.
fn requires_parentheses(arg: Node<'_>) -> bool {
    let assignment = if arg.is_call() {
        arg.is_setter_method()
    } else {
        arg.is_assignment()
    };
    if assignment
        || arg.is_any(&[NodeType::If, NodeType::And, NodeType::Or])
        || arg.is_range()
    {
        return true;
    }
    arg.is(NodeType::Send) && arg.is_operator_method() && !arg.is_parenthesized()
}
