//! Port of `lib/rubocop/cop/style/redundant_percent_q.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::double_quotes_required;
use crate::cop::{Cop, Investigation};
use crate::regex;

const DYNAMIC_MSG: &str = ", or for dynamic strings that contain double quotes";

#[derive(Default)]
pub struct RedundantPercentQ;

impl Cop for RedundantPercentQ {
    fn name(&self) -> &'static str {
        "Style/RedundantPercentQ"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr, NodeType::Str]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !(node.loc().begin.is_some() && node.loc().end.is_some()) {
            return;
        }
        let src = node.source();
        if !(src.starts_with("%q") || src.starts_with("%Q")) {
            return;
        }
        if (src.contains('\'') && src.contains('"')) || allowed_percent_q(node) {
            return;
        }
        let extra = if src.starts_with("%Q") { DYNAMIC_MSG } else { "" };
        let message = format!(
            "Use `{}` only for strings that contain both single quotes and double quotes{extra}.",
            &src[..2]
        );
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            let delimiter = if regex!(r#"\A%Q[^"]+\z|'"#).find(src).is_some() {
                "\""
            } else {
                "'"
            };
            corrector.replace(node.loc().begin.unwrap(), delimiter);
            corrector.replace(node.loc().end.unwrap(), delimiter);
        });
    }
}

fn allowed_percent_q(node: Node<'_>) -> bool {
    let src = node.source();
    (src.starts_with("%q") && acceptable_q(src))
        || (src.starts_with("%Q") && acceptable_capital_q(node))
}

fn acceptable_q(src: &str) -> bool {
    let interpolation = regex!(r"#\{.+\}");
    if interpolation.find(src).is_some() && src.contains('\'') {
        return true;
    }
    regex!(r"\\.")
        .find_iter(src)
        .any(|(b, e)| regex!(r"\\[^\\]").find(&src[b..e]).is_some())
}

fn acceptable_capital_q(node: Node<'_>) -> bool {
    let src = node.source();
    src.contains('"')
        && (regex!(r"#\{.+\}").find(src).is_some()
            || (node.is(NodeType::Str) && double_quotes_required(src)))
}
