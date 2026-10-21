//! Port of `lib/rubocop/cop/style/module_member_existence_check.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static MODULE_MEMBER_INCLUSION: NodePattern = NodePattern::new(
    "(call
      {(call _ %METHODS_WITH_INHERIT_PARAM _?) (call _ %METHODS_WITHOUT_INHERIT_PARAM)}
      {:include? :member?}
      _)",
);

const METHOD_REPLACEMENTS: &[(&str, &str)] = &[
    ("class_variables", "class_variable_defined?"),
    ("instance_methods", "method_defined?"),
    ("private_instance_methods", "private_method_defined?"),
    ("protected_instance_methods", "protected_method_defined?"),
    ("public_instance_methods", "public_method_defined?"),
];
const METHODS_WITHOUT_INHERIT_PARAM: &[&str] = &["class_variables"];
const METHODS_WITH_INHERIT_PARAM: &[&str] = &[
    "instance_methods",
    "private_instance_methods",
    "protected_instance_methods",
    "public_instance_methods",
];

#[derive(Default)]
pub struct ModuleMemberExistenceCheck;

impl Cop for ModuleMemberExistenceCheck {
    fn name(&self) -> &'static str {
        "Style/ModuleMemberExistenceCheck"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "class_variables",
            "instance_methods",
            "private_instance_methods",
            "protected_instance_methods",
            "public_instance_methods",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(parent) = node.parent() else {
            return;
        };
        let ctx = MatchContext {
            named: &[
                ("METHODS_WITH_INHERIT_PARAM", Param::Syms(METHODS_WITH_INHERIT_PARAM)),
                (
                    "METHODS_WITHOUT_INHERIT_PARAM",
                    Param::Syms(METHODS_WITHOUT_INHERIT_PARAM),
                ),
            ],
            ..Default::default()
        };
        if MODULE_MEMBER_INCLUSION.match_with(parent, &ctx).is_none() {
            return;
        }
        if !(simple_method_argument(node) && simple_method_argument(parent)) {
            return;
        }
        let offense_range = node
            .loc()
            .selector
            .unwrap()
            .join(parent.source_range().end());
        let replacement = replacement_for(node, parent);
        let message = format!("Use `{replacement}` instead.");
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            corrector.replace(offense_range, &replacement);
        });
    }
}

fn replacement_for(node: Node<'_>, parent: Node<'_>) -> String {
    let method_name = node.method_name().unwrap_or("");
    let replacement_method = METHOD_REPLACEMENTS
        .iter()
        .find(|(m, _)| *m == method_name)
        .map_or("", |(_, r)| *r);
    let without_inherit_param = METHODS_WITHOUT_INHERIT_PARAM.contains(&method_name);
    let parent_arg = parent.first_argument().unwrap().source();
    match node.first_argument() {
        Some(arg) if !without_inherit_param && !arg.is(NodeType::True) => {
            format!("{replacement_method}({parent_arg}, {})", arg.source())
        }
        _ => format!("{replacement_method}({parent_arg})"),
    }
}

fn simple_method_argument(node: Node<'_>) -> bool {
    if node.is_splat_argument() || node.is_block_argument() {
        return false;
    }
    !node.first_argument().is_some_and(|a| a.is(NodeType::Hash))
}
