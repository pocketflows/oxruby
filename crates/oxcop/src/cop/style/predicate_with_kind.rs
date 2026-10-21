//! Port of `lib/rubocop/cop/style/predicate_with_kind.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Investigation};

const KIND_METHODS: &[&str] = &["is_a?", "kind_of?", "instance_of?"];

#[derive(Default)]
pub struct PredicateWithKind;

impl Cop for PredicateWithKind {
    fn name(&self) -> &'static str {
        "Style/PredicateWithKind"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["any?", "all?", "none?", "one?"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(block_node) = node.block_node() else {
            return;
        };
        if block_node.body().is_some_and(|b| b.is(NodeType::Begin)) {
            return;
        }
        let Some(kind_check_node) = extract_send_node(block_node) else {
            return;
        };
        let klass = kind_check_node.first_argument().unwrap();
        let method_name = node.method_name().unwrap();
        let replacement = format!("{method_name}({})", klass.source());
        let message = format!(
            "Prefer `{replacement}` to `{method_name} {{ ... }}` with a kind check."
        );
        cx.add_offense_with_correction(block_node.source_range(), &message, |corrector| {
            let range = range_between(
                node.loc().selector.unwrap().begin_pos,
                block_node.loc().end.unwrap().end_pos,
            );
            corrector.replace(range, &replacement);
        });
    }
}

/// `(send (lvar _) %KIND_METHODS _)`
fn kind_send(node: Node<'_>) -> bool {
    node.is_send()
        && node.receiver().is_some_and(|r| r.is(NodeType::Lvar))
        && node.method_name().is_some_and(|m| KIND_METHODS.contains(&m))
        && node.arguments().len() == 1
        && node.children().len() == 3
}

/// `kind_check?` then `kind_call?(kind_check_node, block_arg_name)`.
fn extract_send_node(block_node: Node<'_>) -> Option<Node<'_>> {
    let send = block_node.send_node()?;
    if !send.is_call() {
        return None;
    }
    let body = block_node.child(2)?;
    if !kind_send(body) {
        return None;
    }
    let block_arg_name = match block_node.ty() {
        NodeType::Block => {
            let args = block_node.child(1)?;
            if !args.is(NodeType::Args) || args.children().len() != 1 {
                return None;
            }
            let arg = args.child(0)?;
            if !arg.is(NodeType::Arg) {
                return None;
            }
            arg.name()?.to_owned()
        }
        NodeType::Numblock => {
            if block_node.children().get(1) != Some(&Child::Int(1)) {
                return None;
            }
            "_1".to_owned()
        }
        NodeType::Itblock => "it".to_owned(),
        _ => return None,
    };
    (body.receiver()?.name() == Some(block_arg_name.as_str())).then_some(body)
}
