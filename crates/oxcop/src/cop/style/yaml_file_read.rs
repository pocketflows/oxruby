//! Port of `lib/rubocop/cop/style/yaml_file_read.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static YAML_FILE_READ: NodePattern = NodePattern::new(
    "(send
       (const {cbase nil?} :YAML) _
       (send
         (const {cbase nil?} :File) :read $_) $...)",
);

#[derive(Default)]
pub struct YamlFileRead;

impl Cop for YamlFileRead {
    fn name(&self) -> &'static str {
        "Style/YAMLFileRead"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["load", "safe_load", "parse"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_method("safe_load") && cx.target_ruby_version() <= 2.7 {
            return;
        }
        let Some(captures) = YAML_FILE_READ.match_node(node) else {
            return;
        };
        let file_path = captures[0].node().unwrap();
        let rest_arguments = captures[1].nodes();
        let range = node.loc().selector.unwrap().join(node.source_range().end());
        let rest_arguments = if rest_arguments.is_empty() {
            String::new()
        } else {
            format!(
                ", {}",
                rest_arguments
                    .iter()
                    .map(|a| a.source())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let prefer = format!(
            "{}_file({}{rest_arguments})",
            node.method_name().unwrap(),
            file_path.source()
        );
        let message = format!("Use `{prefer}` instead.");
        cx.add_offense_with_correction(range, &message, |corrector| {
            corrector.replace(range, &prefer);
        });
    }
}
