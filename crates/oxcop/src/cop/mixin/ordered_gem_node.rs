//! Port of `lib/rubocop/cop/mixin/ordered_gem_node.rb`: common
//! functionality for Bundler/OrderedGems and Gemspec/OrderedDependencies.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::correctors::ordered_gem_corrector;
use crate::processed_source::{ProcessedSource, Range};

/// `get_source_range(node, comments_as_separators)`.
pub fn get_source_range(
    ps: &ProcessedSource<'_>,
    node: Node<'_>,
    comments_as_separators: bool,
) -> Range {
    if !comments_as_separators && let Some(first_comment) = ps.node_comments(node).first() {
        return first_comment.range;
    }
    node.source_range()
}

fn gem_canonical_name(cx: &Investigation<'_, '_>, name: &str) -> String {
    let name: String = if cx.cop_config_bool("ConsiderPunctuation", false) {
        name.to_owned()
    } else {
        name.chars().filter(|c| !matches!(c, '-' | '_')).collect()
    };
    name.to_lowercase()
}

/// `case_insensitive_out_of_order?(string_a, string_b)`.
pub fn case_insensitive_out_of_order(cx: &Investigation<'_, '_>, a: &str, b: &str) -> bool {
    gem_canonical_name(cx, a) < gem_canonical_name(cx, b)
}

/// `consecutive_lines?(previous, current)`.
pub fn consecutive_lines(
    cx: &Investigation<'_, '_>,
    previous: Node<'_>,
    current: Node<'_>,
) -> bool {
    let ps = cx.processed_source;
    let first_line =
        ps.line_of(get_source_range(ps, current, treat_comments_as_separators(cx)).begin_pos);
    previous.last_line() + 1 == first_line
}

/// `register_offense(previous, current)` with the cop's `MSG`, given as
/// `message(previous: gem_name(current), current: gem_name(previous))`.
pub fn register_offense<'a>(
    cx: &mut Investigation<'a, '_>,
    previous: Node<'a>,
    current: Node<'a>,
    message: impl FnOnce(&str, &str) -> String,
) {
    let message = message(
        gem_name(current).unwrap_or(""),
        gem_name(previous).unwrap_or(""),
    );
    let separators = treat_comments_as_separators(cx);
    cx.add_offense_cx(current.source_range(), &message, |corrector, cx| {
        ordered_gem_corrector::correct(
            cx.processed_source,
            current,
            previous,
            separators,
            corrector,
        );
    });
}

/// `gem_name(declaration_node)`.
pub fn gem_name(declaration_node: Node<'_>) -> Option<&str> {
    find_gem_name(declaration_node.first_argument()?)
}

fn find_gem_name(gem_node: Node<'_>) -> Option<&str> {
    if gem_node.is(NodeType::Str) {
        return gem_node.str_content();
    }
    find_gem_name(gem_node.receiver()?)
}

pub fn treat_comments_as_separators(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("TreatCommentsAsGroupSeparators", false)
}
