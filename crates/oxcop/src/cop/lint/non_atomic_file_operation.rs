//! Port of `lib/rubocop/cop/lint/non_atomic_file_operation.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MAKE_FORCE_METHODS: &[&str] = &["makedirs", "mkdir_p", "mkpath"];
const MAKE_METHODS: &[&str] = &["mkdir"];
const REMOVE_FORCE_METHODS: &[&str] = &["rm_f", "rm_rf"];
const REMOVE_METHODS: &[&str] = &[
    "remove",
    "delete",
    "unlink",
    "remove_file",
    "rm",
    "rmdir",
    "safe_unlink",
];
const RECURSIVE_REMOVE_METHODS: &[&str] = &["remove_dir", "remove_entry", "remove_entry_secure"];

static SEND_EXIST_NODE: NodePattern = NodePattern::new(
    "$(send (const {cbase nil?} {:FileTest :File :Dir :Shell}) {:exist? :exists?} ...)",
);
static RECEIVER_AND_METHOD_NAME: NodePattern = NodePattern::new("(send (const {cbase nil?} $_) $_ ...)");
static FORCE: NodePattern = NodePattern::new("(pair (sym :force) (:true))");
static EXPLICIT_NOT_FORCE: NodePattern = NodePattern::new("(pair (sym :force) (:false))");

#[derive(Default)]
pub struct NonAtomicFileOperation;

impl Cop for NonAtomicFileOperation {
    fn name(&self) -> &'static str {
        "Lint/NonAtomicFileOperation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "mkdir",
            "makedirs",
            "mkdir_p",
            "mkpath",
            "remove",
            "delete",
            "unlink",
            "remove_file",
            "rm",
            "rmdir",
            "safe_unlink",
            "remove_dir",
            "remove_entry",
            "remove_entry_secure",
            "rm_f",
            "rm_rf",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.receiver().is_some_and(|r| r.is(NodeType::Const)) {
            return;
        }
        if !if_node_child(node) {
            return;
        }
        if EXPLICIT_NOT_FORCE.any_in(node) {
            return;
        }
        let Some(exist_node) = node
            .parent()
            .and_then(|p| SEND_EXIST_NODE.search(p).into_iter().next())
            .map(|(n, _)| n)
        else {
            return;
        };
        let same_argument = match (exist_node.first_argument(), node.first_argument()) {
            (Some(a), Some(b)) => a.same_as(b),
            (None, None) => true,
            _ => false,
        };
        if !same_argument {
            return;
        }
        register_offense(cx, node, exist_node);
    }
}

fn if_node_child(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    parent.is(NodeType::If) && !allowable_use_with_if(parent)
}

fn allowable_use_with_if(if_node: Node<'_>) -> bool {
    if_node.condition().is_some_and(|c| c.is_operator_keyword()) || if_node.else_branch().is_some()
}

fn register_offense<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>, exist_node: Node<'a>) {
    if !force_method(node) {
        cx.add_offense(node.source_range(), &message_change_force_method(node));
    }
    let parent = node.parent().unwrap();
    let range = parent
        .loc()
        .keyword
        .unwrap()
        .begin()
        .join(parent.condition().unwrap().source_range().end());
    let message = message_remove_file_exist_check(exist_node);
    cx.add_offense_with_correction(range, &message, |corrector| {
        if !parent.is_elsif() {
            autocorrect(corrector, node, range);
        }
    });
}

fn message_change_force_method(node: Node<'_>) -> String {
    format!(
        "Use atomic file operation method `FileUtils.{}`.",
        replacement_method(node)
    )
}

fn message_remove_file_exist_check(node: Node<'_>) -> String {
    let captures = RECEIVER_AND_METHOD_NAME.match_node(node).unwrap();
    format!(
        "Remove unnecessary existence check `{}.{}`.",
        captures[0].sym().unwrap_or_default(),
        captures[1].sym().unwrap_or_default()
    )
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, range: Range) {
    corrector.remove(range);
    autocorrect_replace_method(corrector, node);
    let parent = node.parent().unwrap();
    if parent.is_modifier_form() {
        corrector.remove(
            node.source_range()
                .end()
                .join(parent.loc().keyword.unwrap().begin()),
        );
    } else {
        corrector.remove(parent.loc().end.unwrap());
    }
}

fn autocorrect_replace_method(corrector: &mut Corrector, node: Node<'_>) {
    if force_method(node) {
        return;
    }
    corrector.replace(
        node.child_nodes().next().unwrap().loc().name.unwrap(),
        "FileUtils",
    );
    corrector.replace(node.loc().selector.unwrap(), replacement_method(node));
    if require_mode_keyword(node) {
        corrector.insert_before(node.last_argument().unwrap().source_range(), "mode: ");
    }
}

fn replacement_method(node: Node<'_>) -> &str {
    let method_name = node.method_name().unwrap();
    if MAKE_METHODS.contains(&method_name) {
        "mkdir_p"
    } else if REMOVE_METHODS.contains(&method_name) {
        "rm_f"
    } else if RECURSIVE_REMOVE_METHODS.contains(&method_name) {
        "rm_rf"
    } else {
        method_name
    }
}

fn force_method(node: Node<'_>) -> bool {
    force_method_name(node) || force_option(node)
}

fn require_mode_keyword(node: Node<'_>) -> bool {
    if node.receiver().and_then(|r| r.const_name()).as_deref() != Some("Dir") {
        return false;
    }
    replacement_method(node) == "mkdir_p" && node.arguments().len() == 2
}

fn force_option(node: Node<'_>) -> bool {
    node.arguments().into_iter().any(|arg| FORCE.any_in(arg))
}

fn force_method_name(node: Node<'_>) -> bool {
    let method_name = node.method_name().unwrap();
    MAKE_FORCE_METHODS.contains(&method_name) || REMOVE_FORCE_METHODS.contains(&method_name)
}
