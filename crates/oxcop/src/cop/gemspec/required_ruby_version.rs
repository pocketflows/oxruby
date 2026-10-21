//! Port of `lib/rubocop/cop/gemspec/required_ruby_version.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::regex;

const MISSING_MSG: &str = "`required_ruby_version` should be specified.";

static REQUIRED_RUBY_VERSION: NodePattern = NodePattern::new("(send _ :required_ruby_version= _)");

static DEFINED_RUBY_VERSION: NodePattern = NodePattern::new(
    "{
       $(str _)
       $(array (str _) (str _))
       (send (const (const nil? :Gem) :Requirement) :new $str+)
     }",
);

#[derive(Default)]
pub struct RequiredRubyVersion;

impl Cop for RequiredRubyVersion {
    fn name(&self) -> &'static str {
        "Gemspec/RequiredRubyVersion"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["required_ruby_version="]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        if cx
            .processed_source
            .ast()
            .is_some_and(|ast| REQUIRED_RUBY_VERSION.any_in(ast))
        {
            return;
        }
        cx.add_global_offense(MISSING_MSG);
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(version_def) = node.first_argument() else {
            return;
        };
        if dynamic_version(version_def) {
            return;
        }
        let ruby_version = extract_ruby_version(version_def);
        let target = target_ruby_version_to_s(cx.target_ruby_version());
        if ruby_version.as_deref() == Some(target.as_str()) {
            return;
        }
        let message = format!(
            "`required_ruby_version` and `TargetRubyVersion` ({target}, which may be specified \
             in .rubocop.yml) should be equal."
        );
        cx.add_offense(version_def.source_range(), &message);
    }
}

/// `Float#to_s` of a target Ruby version (`3.0`, `2.7`).
fn target_ruby_version_to_s(version: f64) -> String {
    let s = version.to_string();
    if s.contains('.') { s } else { format!("{s}.0") }
}

fn dynamic_version(node: Node<'_>) -> bool {
    (node.is(NodeType::Send) && node.receiver().is_none())
        || node.is_variable()
        || node.descendants().any(|d| d.is(NodeType::Send) || d.is_variable())
}

fn extract_ruby_version(version_def: Node<'_>) -> Option<String> {
    let captures = DEFINED_RUBY_VERSION.match_node(version_def)?;
    let capture = captures.first()?;
    let has_version = |v: &Node<'_>| {
        v.str_content()
            .is_some_and(|s| regex!(r"[>=]").find(s).is_some())
    };
    let required = match capture.node() {
        // `$str+`: a list.
        None => capture.nodes().into_iter().find(has_version)?,
        Some(node) if node.is(NodeType::Array) => node.child_nodes().find(has_version)?,
        Some(node) => node,
    };
    let digits: Vec<String> = required
        .str_content()?
        .chars()
        .filter(char::is_ascii_digit)
        .take(2)
        .map(String::from)
        .collect();
    Some(digits.join("."))
}
