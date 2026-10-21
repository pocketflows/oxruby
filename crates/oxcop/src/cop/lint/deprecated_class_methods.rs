//! Port of `lib/rubocop/cop/lint/deprecated_class_methods.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const DIR_ENV_FILE_CONSTANTS: &[&str] = &["Dir", "ENV", "File"];

static DEPRECATED_CLASS_METHOD: NodePattern = NodePattern::new(
    "{
      (send (const {cbase nil?} :ENV) {:clone :dup :freeze})
      (send (const {cbase nil?} {:File :Dir}) :exists? _)
      (send (const {cbase nil?} :Socket) {:gethostbyaddr :gethostbyname} ...)
      (send nil? :attr _ boolean)
      (send nil? :iterator?)
    }",
);

fn preferred_methods(name: &str) -> Option<&'static str> {
    Some(match name {
        "clone" | "dup" => "to_h",
        "exists?" => "exist?",
        "gethostbyaddr" => "Addrinfo#getnameinfo",
        "gethostbyname" => "Addrinfo.getaddrinfo",
        "iterator?" => "block_given?",
        _ => return None,
    })
}

#[derive(Default)]
pub struct DeprecatedClassMethods;

impl Cop for DeprecatedClassMethods {
    fn name(&self) -> &'static str {
        "Lint/DeprecatedClassMethods"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[
            "attr",
            "clone",
            "dup",
            "exists?",
            "freeze",
            "gethostbyaddr",
            "gethostbyname",
            "iterator?",
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !DEPRECATED_CLASS_METHOD.is_match(node) {
            return;
        }
        let offense_range = offense_range(node);
        let prefer = preferred_method(node);
        let message = format!(
            "`{}` is deprecated in favor of `{prefer}`.",
            cx.source(offense_range)
        );
        cx.add_offense_with_correction(offense_range, &message, |corrector| {
            if socket_const(node.receiver()) {
                return;
            }
            if node.is_method("freeze") {
                corrector.replace(node.source_range(), "ENV");
            } else {
                corrector.replace(offense_range, &prefer);
            }
        });
    }
}

fn offense_range(node: Node<'_>) -> Range {
    if socket_const(node.receiver()) || dir_env_file_const(node.receiver()) {
        node.source_range()
            .begin()
            .join(node.loc().selector.unwrap().end())
    } else if node.is_method("attr") {
        node.source_range()
    } else {
        node.loc().selector.unwrap()
    }
}

fn preferred_method(node: Node<'_>) -> String {
    if node.is_method("attr") {
        let boolean_argument = node.arguments()[1].source();
        let preferred_attr_method = if boolean_argument == "true" {
            "attr_accessor"
        } else {
            "attr_reader"
        };
        format!(
            "{preferred_attr_method} {}",
            node.first_argument().unwrap().source()
        )
    } else if dir_env_file_const(node.receiver()) {
        match preferred_methods(node.method_name().unwrap()) {
            Some(prefer) => format!("{}.{prefer}", node.receiver().unwrap().source()),
            None => "ENV".to_owned(),
        }
    } else {
        preferred_methods(node.method_name().unwrap())
            .unwrap_or_default()
            .to_owned()
    }
}

fn socket_const(node: Option<Node<'_>>) -> bool {
    node.and_then(|n| n.short_name()) == Some("Socket")
}

fn dir_env_file_const(node: Option<Node<'_>>) -> bool {
    node.and_then(|n| n.short_name())
        .is_some_and(|name| DIR_ENV_FILE_CONSTANTS.contains(&name))
}
