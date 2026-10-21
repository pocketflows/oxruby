//! Port of `lib/rubocop/cop/naming/class_and_module_camel_case.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Investigation};

const MSG: &str = "Use CamelCase for classes and modules.";

#[derive(Default)]
pub struct ClassAndModuleCamelCase;

impl Cop for ClassAndModuleCamelCase {
    fn name(&self) -> &'static str {
        "Naming/ClassAndModuleCamelCase"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Class, NodeType::Module]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let name_range = node.loc().name.unwrap();
        let source = node.range_source(name_range);
        if !source.contains('_') {
            return;
        }
        let allowed = cx.cop_config_list("AllowedNames").join("|");
        let name = match ruby_regex(&allowed) {
            Some(re) => re.replace_all(source, ""),
            None => source.to_owned(),
        };
        if !name.contains('_') {
            return;
        }
        cx.add_offense(name_range, MSG);
    }
}
