//! Port of `lib/rubocop/cop/lint/uri_regexp.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static URI_CONSTANT: NodePattern = NodePattern::new("(const {cbase nil?} :URI)");

#[derive(Default)]
pub struct UriRegexp;

impl Cop for UriRegexp {
    fn name(&self) -> &'static str {
        "Lint/UriRegexp"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["regexp"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(receiver) = node.receiver().filter(|r| URI_CONSTANT.is_match(*r)) else {
            return;
        };

        let parser = if cx.target_ruby_version() >= 3.4 {
            "RFC2396_PARSER"
        } else {
            "DEFAULT_PARSER"
        };
        let argument = node
            .first_argument()
            .map(|a| format!("({})", a.source()))
            .unwrap_or_default();
        let preferred_method = format!("{}::{parser}.make_regexp{argument}", receiver.source());
        let message = format!(
            "`{}` is obsolete and should not be used. Instead, use `{preferred_method}`.",
            node.source()
        );
        let Some(selector) = node.loc().selector else {
            return;
        };
        cx.add_offense_with_correction(selector, &message, |corrector| {
            corrector.replace(node.source_range(), &preferred_method);
        });
    }
}
