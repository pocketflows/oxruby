//! Port of `lib/rubocop/cop/style/non_nil_check.rb`.

use std::collections::HashSet;

use serde_yaml_ng::Value;

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeId, NodeType};
use crate::cop::{Cop, Corrector, Investigation};

const MSG_FOR_REDUNDANCY: &str = "Explicit non-nil checks are usually redundant.";

static NOT_EQUAL_TO_NIL: NodePattern = NodePattern::new("(send _ :!= nil)");
static UNLESS_CHECK: NodePattern = NodePattern::new("(if (send _ :nil?) ...)");
static NIL_CHECK: NodePattern = NodePattern::new("(send _ :nil?)");
static NOT_AND_NIL_CHECK: NodePattern = NodePattern::new("(send (send _ :nil?) :!)");

#[derive(Default)]
pub struct NonNilCheck {
    /// `ignore_node` / `ignored_node?` (identity, so per investigation).
    ignored: HashSet<NodeId>,
}

impl Cop for NonNilCheck {
    fn name(&self) -> &'static str {
        "Style/NonNilCheck"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Def, NodeType::Defs]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["!=", "nil?", "!"]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.ignored.clear();
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_any_def() {
            self.on_def(node);
            return;
        }
        let include_semantic_changes = cx.cop_config_bool("IncludeSemanticChanges", false);
        if self.ignored.contains(&node.id())
            || (!include_semantic_changes && nil_comparison_style(cx) == Some("comparison"))
        {
            return;
        }
        if !register_offense(node, include_semantic_changes) {
            return;
        }
        let message = if node.is_method("!=") && !include_semantic_changes {
            format!(
                "Prefer `{}` over `{}`.",
                non_nil_check_replacement(node),
                node.source()
            )
        } else {
            MSG_FOR_REDUNDANCY.to_owned()
        };
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            autocorrect(corrector, node, include_semantic_changes);
        });
    }
}

impl NonNilCheck {
    fn on_def(&mut self, node: Node<'_>) {
        let Some(body) = node.body() else {
            return;
        };
        if !node.is_predicate_method() {
            return;
        }
        if body.is(NodeType::Begin) {
            if let Some(last) = body.child_nodes().last() {
                self.ignored.insert(last.id());
            }
        } else {
            self.ignored.insert(body.id());
        }
    }
}

fn register_offense(node: Node<'_>, include_semantic_changes: bool) -> bool {
    NOT_EQUAL_TO_NIL.is_match(node)
        || (include_semantic_changes
            && (NOT_AND_NIL_CHECK.is_match(node) || unless_and_nil_check(node)))
}

fn unless_and_nil_check(send_node: Node<'_>) -> bool {
    let Some(parent) = send_node.parent() else {
        return false;
    };
    NIL_CHECK.is_match(send_node)
        && UNLESS_CHECK.is_match(parent)
        && !parent.is_ternary()
        && parent.is_unless()
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, include_semantic_changes: bool) {
    match node.method_name() {
        Some("!=") => {
            if include_semantic_changes {
                corrector.replace(node.source_range(), node.receiver().unwrap().source());
            } else {
                corrector.replace(node.source_range(), non_nil_check_replacement(node));
            }
        }
        Some("!") => {
            let inner = node.receiver().unwrap();
            match inner.receiver() {
                Some(receiver) => corrector.replace(node.source_range(), receiver.source()),
                None => corrector.replace(node.source_range(), "self"),
            }
        }
        Some("nil?") => {
            corrector.replace(node.parent().unwrap().loc().keyword.unwrap(), "if");
            corrector.replace(node.source_range(), node.receiver().unwrap().source());
        }
        _ => {}
    }
}

fn non_nil_check_replacement(node: Node<'_>) -> String {
    let receiver = node.receiver().unwrap();
    if operator_expression(receiver) {
        format!("!({}).nil?", receiver.source())
    } else {
        format!("!{}.nil?", receiver.source())
    }
}

fn operator_expression(node: Node<'_>) -> bool {
    node.is_operator_keyword()
        || (node.is_send() && node.is_binary_operation())
        || (node.is(NodeType::If) && node.is_ternary())
        || matches!(
            node.ty(),
            NodeType::Irange | NodeType::Erange | NodeType::Iflipflop | NodeType::Eflipflop
        )
        || node.is_assignment()
}

/// `nil_comparison_conf['Enabled'] && nil_comparison_conf['EnforcedStyle']`,
/// where `Config#for_cop` fills in `Enabled` with `enable_cop?`.
fn nil_comparison_style<'a>(cx: &Investigation<'a, '_>) -> Option<&'a str> {
    let name = "Style/NilComparison";
    let conf = cx.config.for_cop(name);
    let enabled = match conf.get("Enabled") {
        Some(Value::Bool(true)) => true,
        Some(Value::String(s)) if s == "override_department" => true,
        _ if cx.config.for_department("Style").get("Enabled") == Some(&Value::Bool(false)) => false,
        None => !cx.config.all_cops_bool("DisabledByDefault", false),
        Some(Value::Bool(false)) | Some(Value::Null) => false,
        Some(_) => true,
    };
    if enabled {
        conf.get("EnforcedStyle").and_then(Value::as_str)
    } else {
        None
    }
}
