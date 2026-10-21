//! Port of `lib/rubocop/cop/style/class_equality_comparison.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_method;
use crate::cop::mixin::allowed_pattern::matches_allowed_pattern;
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const CLASS_NAME_METHODS: &[&str] = &["name", "to_s", "inspect"];

static CLASS_COMPARISON_CANDIDATE: NodePattern = NodePattern::new(
    "(send
       {$(send _ :class) (send $(send _ :class) #class_name_method?)}
       {:== :equal? :eql?} $_)",
);

#[derive(Default)]
pub struct ClassEqualityComparison;

impl Cop for ClassEqualityComparison {
    fn name(&self) -> &'static str {
        "Style/ClassEqualityComparison"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["==", "equal?", "eql?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(def_node) = node.ancestors().find(|a| a.is_any_def()) {
            let method_name = def_node.method_name().unwrap_or("");
            if allowed_method(cx, method_name) || matches_allowed_pattern(cx, method_name) {
                return;
            }
        }
        let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
            name == "class_name_method?" && val.sym().is_some_and(class_name_method)
        };
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        let Some(captures) = CLASS_COMPARISON_CANDIDATE.match_with(node, &ctx) else {
            return;
        };
        let (Some(receiver_node), Some(class_node)) = (captures[0].node(), captures[1].node())
        else {
            return;
        };
        if class_node.is(NodeType::Dstr) {
            return;
        }
        let range = offense_range(receiver_node, node);
        let class_name = class_name(class_node, node);
        let class_argument = class_name
            .as_ref()
            .map_or(String::new(), |name| format!("({name})"));
        let message = format!("Use `instance_of?{class_argument}` instead of comparing classes.");
        cx.add_offense_with_correction(range, &message, |corrector| {
            if class_name.is_none() {
                return;
            }
            corrector.replace(range, format!("instance_of?{class_argument}"));
        });
    }
}

fn class_name_method(method_name: &str) -> bool {
    CLASS_NAME_METHODS.contains(&method_name)
}

fn class_name(class_node: Node<'_>, node: Node<'_>) -> Option<String> {
    let first = node.child(0).unwrap();
    if !class_name_method(first.method_name().unwrap_or("")) {
        if class_node.is(NodeType::Str) {
            return None;
        }
        return Some(class_node.source().to_owned());
    }
    if let Some(receiver) = class_node.receiver()
        && class_node
            .method_name()
            .is_some_and(class_name_method)
    {
        return Some(receiver.source().to_owned());
    }
    if class_node.is(NodeType::Str) {
        return Some(string_class_name(class_node));
    }
    if class_node.is_variable() || class_node.is_call() {
        return None;
    }
    Some(class_node.source().to_owned())
}

fn string_class_name(class_node: Node<'_>) -> String {
    let value: String = class_node
        .source()
        .chars()
        .filter(|c| *c != '"' && *c != '\'')
        .collect();
    if require_cbase(class_node) && !value.starts_with("::") {
        format!("::{value}")
    } else {
        value
    }
}

fn require_cbase(class_node: Node<'_>) -> bool {
    class_node
        .ancestors()
        .any(|a| matches!(a.ty(), NodeType::Class | NodeType::Module))
}

fn offense_range(receiver_node: Node<'_>, node: Node<'_>) -> Range {
    range_between(
        receiver_node.loc().selector.unwrap().begin_pos,
        node.source_range().end_pos,
    )
}
