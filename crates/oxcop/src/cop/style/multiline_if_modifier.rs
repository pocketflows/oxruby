//! Port of `lib/rubocop/cop/style/multiline_if_modifier.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::{indentation, offset};
use crate::cop::mixin::ignored_node::IgnoredNodes;
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct MultilineIfModifier {
    ignored: IgnoredNodes,
}

impl Cop for MultilineIfModifier {
    fn name(&self) -> &'static str {
        "Style/MultilineIfModifier"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::If]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if self.ignored.part_of(node) {
            return;
        }
        if !(node.is_modifier_form() && node.body().is_some_and(|b| b.is_multiline())) {
            return;
        }
        let message = format!(
            "Favor a normal {}-statement over a modifier clause in a multiline statement.",
            node.keyword()
        );
        cx.add_offense_cx(node.source_range(), &message, |corrector, cx| {
            corrector.replace(node.source_range(), to_normal_if(cx, node));
        });
        self.ignored.ignore(node);
    }
}

fn to_normal_if(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let indented_body = indented_body(cx, node.body().unwrap(), node);
    let condition = format!("{} {}", node.keyword(), node.condition().unwrap().source());
    let indented_end = format!("{}end", offset(node));
    [condition, indented_body, indented_end].join("\n")
}

fn indented_body(cx: &Investigation<'_, '_>, body: Node<'_>, node: Node<'_>) -> String {
    let offset = offset(node);
    let body_source = format!("{offset}{}", body.source());
    let indentation = indentation(cx, node);
    let re = crate::cop::mixin::ruby_regex(&format!(r"^\s{{{}}}", offset.len())).unwrap();
    body_source
        .split_inclusive('\n')
        .map(|line| {
            if line == "\n" {
                line.to_owned()
            } else {
                match re.find(line) {
                    Some((b, e)) => format!("{}{indentation}{}", &line[..b], &line[e..]),
                    None => line.to_owned(),
                }
            }
        })
        .collect()
}
