//! Port of `lib/rubocop/cop/security/yaml_load.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Prefer using `YAML.safe_load` over `YAML.load`.";

static YAML_LOAD: NodePattern = NodePattern::new("(send (const {nil? cbase} :YAML) :load ...)");

#[derive(Default)]
pub struct YamlLoad;

impl Cop for YamlLoad {
    fn name(&self) -> &'static str {
        "Security/YAMLLoad"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn maximum_target_ruby_version(&self) -> Option<f64> {
        Some(3.0)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["load"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !YAML_LOAD.is_match(node) {
            return;
        }
        let selector = node.loc().selector.unwrap();
        cx.add_offense_with_correction(selector, MSG, |corrector| {
            corrector.replace(selector, "safe_load");
        });
    }
}
