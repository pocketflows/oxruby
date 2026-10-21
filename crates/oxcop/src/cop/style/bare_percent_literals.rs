//! Port of `lib/rubocop/cop/style/bare_percent_literals.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::configurable_enforced_style::style;
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct BarePercentLiterals;

impl Cop for BarePercentLiterals {
    fn name(&self) -> &'static str {
        "Style/BarePercentLiterals"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Dstr, NodeType::Str]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_heredoc() {
            return;
        }
        let Some(location) = node.loc().begin else {
            return;
        };
        let source = node.range_source(location);
        let style = style(cx);
        let (good, bad) = if style == "percent_q" && regex!(r"^%[^\w]").find(source).is_some() {
            ("Q", "")
        } else if style == "bare_percent" && source.starts_with("%Q") {
            ("", "Q")
        } else {
            return;
        };
        let message = format!("Use `%{good}` instead of `%{bad}`.");
        cx.add_offense_with_correction(location, &message, |corrector| {
            let replacement = if source.starts_with("%Q") { "%" } else { "%Q" };
            corrector.replace(location, regex!(r"%Q?").replace(source, replacement));
        });
    }
}
