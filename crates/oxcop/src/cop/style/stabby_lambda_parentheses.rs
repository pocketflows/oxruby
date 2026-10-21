//! Port of `lib/rubocop/cop/style/stabby_lambda_parentheses.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};

const MSG_REQUIRE: &str = "Wrap stabby lambda arguments with parentheses.";
const MSG_NO_REQUIRE: &str = "Do not wrap stabby lambda arguments with parentheses.";

#[derive(Default)]
pub struct StabbyLambdaParentheses;

impl Cop for StabbyLambdaParentheses {
    fn name(&self) -> &'static str {
        "Style/StabbyLambdaParentheses"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["lambda"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.is_lambda_literal() && node.block_node().is_some_and(|b| b.has_arguments())) {
            return;
        }
        let arguments = node.block_node().unwrap().arguments_node().unwrap();
        let parentheses = arguments.loc().begin.is_some();
        let style = style(cx);
        let require = style == "require_parentheses";
        let missing = require && !parentheses;
        let redundant = style == "require_no_parentheses" && parentheses;
        if !(redundant || missing) {
            return;
        }
        let message = if require { MSG_REQUIRE } else { MSG_NO_REQUIRE };
        cx.add_offense_with_correction(arguments.source_range(), message, |corrector| {
            match style {
                "require_parentheses" => {
                    corrector.wrap(arguments.source_range(), "(", ")");
                }
                "require_no_parentheses" => {
                    corrector.replace(arguments.loc().begin.unwrap(), "");
                    corrector.remove(arguments.loc().end.unwrap());
                }
                _ => {}
            }
        });
    }
}
