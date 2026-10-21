//! Port of `lib/rubocop/cop/lint/require_relative_self_path.rb`.

use std::path::Path;

use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Remove the `require_relative` that requires itself.";

#[derive(Default)]
pub struct RequireRelativeSelfPath;

impl Cop for RequireRelativeSelfPath {
    fn name(&self) -> &'static str {
        "Lint/RequireRelativeSelfPath"
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
        let Some(required_feature) = node.first_argument() else {
            return;
        };
        // `respond_to?(:value)`: only a string value can equal the path.
        let Some(value) = required_feature.str_content() else {
            return;
        };
        if !same_file(&cx.processed_source.file_path(), value) {
            return;
        }

        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
            corrector.remove(range_by_whole_lines(ps, node.source_range(), true));
        });
    }
}

fn same_file(file_path: &str, required_feature: &str) -> bool {
    let path = Path::new(file_path);
    if path.extension().and_then(|e| e.to_str()) != Some("rb") {
        return false;
    }
    let Some(file_name) = path.file_name().and_then(|f| f.to_str()) else {
        return false;
    };
    let basename = file_name.strip_suffix(".rb").unwrap_or(file_name);
    required_feature == basename || required_feature == format!("{basename}.rb")
}
