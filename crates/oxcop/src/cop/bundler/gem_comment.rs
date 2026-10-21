//! Port of `lib/rubocop/cop/bundler/gem_comment.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::gem_declaration::gem_declaration;
use crate::cop::mixin::util::comment_line;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Missing gem description comment.";
const CHECKED_OPTIONS_CONFIG: &str = "OnlyFor";
const VERSION_SPECIFIERS_OPTION: &str = "version_specifiers";
const RESTRICTIVE_VERSION_SPECIFIERS_OPTION: &str = "restrictive_version_specifiers";

#[derive(Default)]
pub struct GemComment;

impl Cop for GemComment {
    fn name(&self) -> &'static str {
        "Bundler/GemComment"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["gem"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !gem_declaration(node) || ignored_gem(cx, node) || commented_any_descendant(cx, node) {
            return;
        }
        if !cx.cop_config_list(CHECKED_OPTIONS_CONFIG).is_empty() && !checked_options_present(cx, node) {
            return;
        }
        cx.add_offense(node.source_range(), MSG);
    }
}

fn commented_any_descendant(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    commented(cx, node) || node.descendants().any(|n| commented(cx, n))
}

fn commented(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let ps = cx.processed_source;
    let line = node.first_line();
    // `preceding_lines(node).last`: the last associated comment that starts
    // on or before the node's line.
    let Some(comment) = ps
        .node_comments(node)
        .into_iter()
        .filter(|c| ps.line_of(c.range.begin_pos) <= line)
        .last()
    else {
        return false;
    };
    // `precede?(comment, node)` and `comment_line?(comment.source)`.
    line.saturating_sub(ps.line_of(comment.range.begin_pos)) <= 1
        && comment_line(ps.slice(comment.range))
}

fn ignored_gem(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let name = node.first_argument().and_then(|a| a.str_content()).unwrap_or("");
    cx.cop_config_list("AllowedGems").contains(&name)
}

fn checked_options_present(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let checked = cx.cop_config_list(CHECKED_OPTIONS_CONFIG);
    (checked.contains(&VERSION_SPECIFIERS_OPTION) && version_specified_gem(node))
        || (checked.contains(&RESTRICTIVE_VERSION_SPECIFIERS_OPTION)
            && restrictive_version_specified_gem(node))
        || contains_checked_options(&checked, node)
}

/// Besides the gem name, all other *positional* arguments to `gem` are
/// version specifiers.
fn version_specified_gem(node: Node<'_>) -> bool {
    node.arguments().get(1).is_some_and(|a| a.is(NodeType::Str))
}

/// Version specifications that restrict all updates going forward. This
/// excludes versions like ">= 1.0" or "!= 2.0.3".
fn restrictive_version_specified_gem(node: Node<'_>) -> bool {
    if !version_specified_gem(node) {
        return false;
    }
    node.arguments()[1..].iter().any(|arg| {
        arg.is(NodeType::Str)
            && arg
                .str_content()
                .is_some_and(|v| regex!(r"\A\s*(?:<|~>|\d|=)").find(v).is_some())
    })
}

fn contains_checked_options(checked: &[&str], node: Node<'_>) -> bool {
    gem_options(node).iter().any(|option| checked.contains(&option.as_str()))
}

fn gem_options(node: Node<'_>) -> Vec<String> {
    let Some(last) = node.last_argument().filter(|a| a.is(NodeType::Hash)) else {
        return Vec::new();
    };
    // Only literal keys carry an option name to check.
    last.hash_keys()
        .iter()
        .filter(|k| matches!(k.ty(), NodeType::Sym | NodeType::Str))
        .filter_map(|k| match k.children().first() {
            Some(Child::Sym(s) | Child::Str(s)) => Some(s.clone()),
            _ => None,
        })
        .collect()
}
