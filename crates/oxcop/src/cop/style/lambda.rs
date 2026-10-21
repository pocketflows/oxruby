//! Port of `lib/rubocop/cop/style/lambda.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::correctors::lambda_literal_to_method_corrector::LambdaLiteralToMethodCorrector;
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Corrector, Investigation};

#[derive(Default)]
pub struct Lambda;

impl Cop for Lambda {
    fn name(&self) -> &'static str {
        "Style/Lambda"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/SymbolProc"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block, NodeType::Numblock, NodeType::Itblock]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_lambda() {
            return;
        }
        let send_node = node.send_node().unwrap();
        let selector = send_node.source();
        let style = style(cx);
        if !offending_selector(style, node, selector) {
            return;
        }
        let message = message(style, node, selector);
        let source = cx.processed_source.buffer_source();
        cx.add_offense_with_correction(send_node.source_range(), &message, |corrector| {
            if send_node.is_lambda_literal() {
                LambdaLiteralToMethodCorrector::new(node, source).call(corrector);
            } else {
                autocorrect_method_to_literal(corrector, node);
            }
        });
    }
}

/// `OFFENDING_SELECTORS[:style][style][lines]`.
fn offending_selector(style: &str, node: Node<'_>, selector: &str) -> bool {
    let multiline = node.is_multiline();
    let offending = match style {
        "lambda" => "->",
        "literal" => "lambda",
        "line_count_dependent" => {
            if multiline {
                "->"
            } else {
                "lambda"
            }
        }
        _ => return false,
    };
    selector == offending
}

fn message(style: &str, node: Node<'_>, selector: &str) -> String {
    let modifier = message_line_modifier(style, node);
    if selector == "->" {
        format!("Use the `lambda` method for {modifier} lambdas.")
    } else {
        format!("Use the `-> {{ ... }}` lambda literal syntax for {modifier} lambdas.")
    }
}

fn message_line_modifier(style: &str, node: Node<'_>) -> &'static str {
    match style {
        "line_count_dependent" => {
            if node.is_multiline() {
                "multiline"
            } else {
                "single line"
            }
        }
        _ => "all",
    }
}

fn autocorrect_method_to_literal(corrector: &mut Corrector, node: Node<'_>) {
    let send_node = node.send_node().unwrap();
    corrector.replace(send_node.source_range(), "->");
    if !node.has_arguments() {
        return;
    }
    let args = node.child(1).unwrap();
    let arg_str = format!("({})", lambda_arg_string(args));
    corrector.insert_after(send_node.source_range(), arg_str);
    corrector.remove(arguments_with_whitespace(node, args));
}

fn arguments_with_whitespace(node: Node<'_>, args: Node<'_>) -> crate::processed_source::Range {
    node.loc()
        .begin
        .unwrap()
        .end()
        .join(args.loc().end.unwrap())
}

fn lambda_arg_string(args: Node<'_>) -> String {
    // Block-local (shadow) arguments are separated from regular arguments by
    // a `;`; joining everything with `,` would turn them into extra
    // parameters and change the lambda's arity.
    let (shadow, regular): (Vec<Node<'_>>, Vec<Node<'_>>) =
        args.child_nodes().partition(|arg| arg.is(NodeType::Shadowarg));
    let mut arg_string = regular
        .iter()
        .map(|a| a.source())
        .collect::<Vec<_>>()
        .join(", ");
    if !shadow.is_empty() {
        arg_string.push_str("; ");
        arg_string.push_str(
            &shadow
                .iter()
                .map(|a| a.source())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    arg_string
}
