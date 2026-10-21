//! Port of `lib/rubocop/cop/style/file_null.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::regex;

#[derive(Default)]
pub struct FileNull {
    contain_dev_null_string_in_file: bool,
}

impl Cop for FileNull {
    fn name(&self) -> &'static str {
        "Style/FileNull"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str]
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        self.contain_dev_null_string_in_file = false;
        let Some(ast) = cx.processed_source.ast() else {
            return;
        };
        self.contain_dev_null_string_in_file = ast.descendants().any(|str| {
            str.is(NodeType::Str)
                && str
                    .str_content()
                    .is_some_and(|content| valid_string(content) && content.to_lowercase() == "/dev/null")
        });
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(value) = node.str_content() else {
            return;
        };
        if !valid_string(value) || acceptable(node) {
            return;
        }
        if value.to_lowercase() == "nul" && !self.contain_dev_null_string_in_file {
            return;
        }
        if !regex!(r"(?i)\A(/dev/null|NUL:?)\z").is_match(value) {
            return;
        }
        let message = format!("Use `File::NULL` instead of `{value}`.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), "File::NULL");
        });
    }
}

fn valid_string(value: &str) -> bool {
    !value.is_empty()
}

fn acceptable(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is_any(&[NodeType::Array, NodeType::Pair, NodeType::Dstr]))
}
