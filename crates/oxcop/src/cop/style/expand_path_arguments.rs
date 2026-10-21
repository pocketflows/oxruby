//! Port of `lib/rubocop/cop/style/expand_path_arguments.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::arguments_range;
use crate::cop::{Corrector, Cop, Investigation};

const PATHNAME_MSG: &str =
    "Use `Pathname(__dir__).expand_path` instead of `Pathname(__FILE__).parent.expand_path`.";
const PATHNAME_NEW_MSG: &str = "Use `Pathname.new(__dir__).expand_path` instead of `Pathname.new(__FILE__).parent.expand_path`.";

static FILE_EXPAND_PATH: NodePattern = NodePattern::new(
    "(send
       (const {nil? cbase} :File) :expand_path
       $_
       $_)",
);

static PATHNAME_PARENT_EXPAND_PATH: NodePattern = NodePattern::new(
    "(send
       (send
         (send nil? :Pathname
           $_) :parent) :expand_path)",
);

static PATHNAME_NEW_PARENT_EXPAND_PATH: NodePattern = NodePattern::new(
    "(send
       (send
         (send
           (const {nil? cbase} :Pathname) :new
           $_) :parent) :expand_path)",
);

#[derive(Default)]
pub struct ExpandPathArguments;

impl Cop for ExpandPathArguments {
    fn name(&self) -> &'static str {
        "Style/ExpandPathArguments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["expand_path"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if let Some(captures) = FILE_EXPAND_PATH.match_node(node) {
            let (Some(current_path), Some(default_dir)) = (captures[0].node(), captures[1].node())
            else {
                return;
            };
            inspect_offense_for_expand_path(cx, node, current_path, default_dir);
        } else if let Some(default_dir) = first(&PATHNAME_PARENT_EXPAND_PATH, node) {
            if !unrecommended_argument(default_dir) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), PATHNAME_MSG, |corrector| {
                autocorrect(corrector, node);
            });
        } else if let Some(default_dir) = first(&PATHNAME_NEW_PARENT_EXPAND_PATH, node) {
            if !unrecommended_argument(default_dir) {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), PATHNAME_NEW_MSG, |corrector| {
                autocorrect(corrector, node);
            });
        }
    }
}

fn first<'a>(pattern: &NodePattern, node: Node<'a>) -> Option<Node<'a>> {
    pattern.match_node(node).and_then(|c| c[0].node())
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    if let Some(captures) = FILE_EXPAND_PATH.match_node(node) {
        autocorrect_expand_path(
            corrector,
            captures[0].node().unwrap(),
            captures[1].node().unwrap(),
        );
    } else if let Some(dir) = first(&PATHNAME_PARENT_EXPAND_PATH, node)
        .or_else(|| first(&PATHNAME_NEW_PARENT_EXPAND_PATH, node))
    {
        corrector.replace(dir.source_range(), "__dir__");
        remove_parent_method(corrector, dir);
    }
}

fn unrecommended_argument(default_dir: Node<'_>) -> bool {
    default_dir.source() == "__FILE__"
}

fn inspect_offense_for_expand_path(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    current_path: Node<'_>,
    default_dir: Node<'_>,
) {
    if !unrecommended_argument(default_dir) || !current_path.is(NodeType::Str) {
        return;
    }
    let stripped = strip_surrounded_quotes(current_path.source());
    let parent_path = parent_path(stripped);
    let new_path = if parent_path.is_empty() {
        String::new()
    } else {
        format!("'{parent_path}', ")
    };
    let new_default_dir = if depth(stripped) == 0 {
        "__FILE__"
    } else {
        "__dir__"
    };
    let message = format!(
        "Use `expand_path({new_path}{new_default_dir})` instead of `expand_path('{stripped}', __FILE__)`."
    );
    cx.add_offense_with_correction(node.loc().selector.unwrap(), &message, |corrector| {
        autocorrect(corrector, node);
    });
}

fn autocorrect_expand_path(corrector: &mut Corrector, current_path: Node<'_>, default_dir: Node<'_>) {
    let stripped = strip_surrounded_quotes(current_path.source());
    match depth(stripped) {
        0 => corrector.replace(arguments_range(current_path.parent().unwrap()), "__FILE__"),
        1 => corrector.replace(arguments_range(current_path.parent().unwrap()), "__dir__"),
        _ => {
            corrector.replace(
                current_path.source_range(),
                format!("'{}'", parent_path(stripped)),
            );
            corrector.replace(default_dir.source_range(), "__dir__");
        }
    }
}

fn strip_surrounded_quotes(path: &str) -> &str {
    let mut chars = path.chars();
    chars.next();
    chars.next_back();
    chars.as_str()
}

/// `String#split('/')`: trailing empty fields are dropped.
fn split_path(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path.split('/').collect();
    while parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

fn depth(current_path: &str) -> usize {
    split_path(current_path)
        .iter()
        .filter(|p| **p != ".")
        .count()
}

fn parent_path(current_path: &str) -> String {
    let mut paths: Vec<&str> = split_path(current_path)
        .into_iter()
        .filter(|p| *p != ".")
        .collect();
    if let Some(index) = paths.iter().position(|p| *p == "..") {
        paths.remove(index);
    }
    paths.join("/")
}

fn remove_parent_method(corrector: &mut Corrector, default_dir: Node<'_>) {
    let node = default_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .and_then(|p| p.child(0))
        .unwrap();
    corrector.remove(node.loc().dot.unwrap());
    corrector.remove(node.loc().selector.unwrap());
}
