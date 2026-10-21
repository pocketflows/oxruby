//! Port of `lib/rubocop/cop/style/concat_array_literals.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::ruby_inspect;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

#[derive(Default)]
pub struct ConcatArrayLiterals;

impl Cop for ConcatArrayLiterals {
    fn name(&self) -> &'static str {
        "Style/ConcatArrayLiterals"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["concat"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let arguments = node.arguments();
        if arguments.is_empty() || !arguments.iter().all(|a| a.is(NodeType::Array)) {
            return;
        }
        let offense = offense_range(node);
        let current = cx.source(offense);
        let use_percent_literal = arguments.iter().any(|a| a.is_percent_literal());
        let mut prefer = None;
        let message = if use_percent_literal {
            if percent_literals_includes_only_basic_literals(&arguments) {
                let p = preferred_method(&arguments);
                let message = format!("Use `{p}` instead of `{current}`.");
                prefer = Some(p);
                message
            } else {
                format!(
                    "Use `push` with elements as arguments without array brackets instead of `{current}`."
                )
            }
        } else {
            let p = preferred_method(&arguments);
            let message = format!("Use `{p}` instead of `{current}`.");
            prefer = Some(p);
            message
        };
        cx.add_offense_with_correction(offense, &message, |corrector| {
            if use_percent_literal {
                if let Some(prefer) = prefer {
                    corrector.replace(offense, prefer);
                }
            } else if arguments.iter().any(|a| a.children().is_empty()) {
                corrector.replace(offense, preferred_method(&arguments));
            } else {
                corrector.replace(node.loc().selector.unwrap(), "push");
                for argument in &arguments {
                    corrector.remove(argument.loc().begin.unwrap());
                    corrector.remove(argument.loc().end.unwrap());
                }
            }
        });
    }
}

fn offense_range(node: Node<'_>) -> Range {
    node.loc().selector.unwrap().join(node.source_range().end())
}

fn preferred_method(arguments: &[Node<'_>]) -> String {
    let new_arguments: Vec<String> = arguments
        .iter()
        .flat_map(|arg| {
            let percent = arg.is_percent_literal();
            arg.child_nodes()
                .map(move |child| {
                    if percent {
                        inspect_value(child)
                    } else {
                        child.source().to_owned()
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    format!("push({})", new_arguments.join(", "))
}

/// `child.value.inspect` for the elements of `%w` and `%i` arrays.
fn inspect_value(child: Node<'_>) -> String {
    match child.ty() {
        NodeType::Sym => format!(":{}", child.sym_value().unwrap_or("")),
        NodeType::Str => ruby_inspect(child.str_content().unwrap_or("")),
        _ => child.source().to_owned(),
    }
}

fn percent_literals_includes_only_basic_literals(arguments: &[Node<'_>]) -> bool {
    arguments
        .iter()
        .filter(|a| a.is_percent_literal())
        .all(|arg| {
            arg.child_nodes()
                .all(|child| matches!(child.ty(), NodeType::Str | NodeType::Sym))
        })
}
