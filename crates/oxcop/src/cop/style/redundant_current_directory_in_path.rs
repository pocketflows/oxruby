//! Port of `lib/rubocop/cop/style/redundant_current_directory_in_path.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::regex;

const MSG: &str = "Remove the redundant current directory path.";

#[derive(Default)]
pub struct RedundantCurrentDirectoryInPath;

impl Cop for RedundantCurrentDirectoryInPath {
    fn name(&self) -> &'static str {
        "Style/RedundantCurrentDirectoryInPath"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["require_relative"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(first_argument) = node.first_argument() else {
            return;
        };
        let Some((index, _)) = regex!(r"\./+").find(first_argument.source()) else {
            return;
        };
        let Some(content) = leading_path_content(first_argument) else {
            return;
        };
        let Some((_, redundant_length)) = regex!(r"\A\./+").find(content) else {
            return;
        };
        let begin_pos = first_argument.source_range().begin_pos + index;
        let range = range_between(begin_pos, begin_pos + redundant_length);
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.remove(range);
        });
    }
}

/// The literal text at the start of the path.
fn leading_path_content(node: Node<'_>) -> Option<&str> {
    if node.is(NodeType::Str) {
        return node.str_content();
    }
    if !node.is(NodeType::Dstr) {
        return None;
    }
    let first = node.child(0)?;
    first.str_content()
}
