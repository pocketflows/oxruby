//! Port of `lib/rubocop/cop/style/nested_file_dirname.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

static FILE_DIRNAME: NodePattern = NodePattern::new("(send (const {cbase nil?} :File) :dirname ...)");

#[derive(Default)]
pub struct NestedFileDirname;

impl Cop for NestedFileDirname {
    fn name(&self) -> &'static str {
        "Style/NestedFileDirname"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(3.1)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["dirname"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.parent().is_some_and(|p| FILE_DIRNAME.is_match(p))
            || !node.first_argument().is_some_and(|a| FILE_DIRNAME.is_match(a))
        {
            return;
        }
        let (path, level) = path_with_dir_level(node, 1);
        if level < 2 {
            return;
        }
        let message = format!("Use `dirname({path}, {level})` instead.");
        let range = range_between(
            node.loc().selector.unwrap().begin_pos,
            node.source_range().end_pos,
        );
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, format!("dirname({path}, {level})"));
        });
    }
}

fn path_with_dir_level(node: Node<'_>, level: usize) -> (&str, usize) {
    let first_argument = node.first_argument().unwrap();
    if FILE_DIRNAME.is_match(first_argument) {
        path_with_dir_level(first_argument, level + 1)
    } else {
        (first_argument.source(), level)
    }
}
