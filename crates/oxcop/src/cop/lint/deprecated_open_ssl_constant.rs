//! Port of `lib/rubocop/cop/lint/deprecated_open_ssl_constant.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_between;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const NO_ARG_ALGORITHM: &[&str] = &["BF", "DES", "IDEA", "RC4"];

static ALGORITHM_CONST: NodePattern = NodePattern::new(
    "(send
      $(const
        (const
          (const {nil? cbase} :OpenSSL) {:Cipher :Digest})
        _)
      ...)",
);
static DIGEST_CONST: NodePattern = NodePattern::new("(const _ :Digest)");

#[derive(Default)]
/// Named after the file (`build.rs`); the cop is `Lint/DeprecatedOpenSSLConstant`.
pub struct DeprecatedOpenSslConstant;

impl Cop for DeprecatedOpenSslConstant {
    fn name(&self) -> &'static str {
        "Lint/DeprecatedOpenSSLConstant"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new", "digest"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node
            .arguments()
            .iter()
            .any(|arg| arg.is_variable() || arg.is_call() || arg.is(NodeType::Const))
        {
            return;
        }
        if node.receiver().is_some_and(|r| DIGEST_CONST.is_match(r)) {
            return;
        }
        let Some(algorithm_constant) = algorithm_const(node) else {
            return;
        };
        let message = message(node, algorithm_constant);
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            autocorrect(corrector, node, algorithm_constant);
        });
    }
}

fn algorithm_const(node: Node<'_>) -> Option<Node<'_>> {
    ALGORITHM_CONST.match_node(node).and_then(|c| c[0].node())
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, algorithm_constant: Node<'_>) {
    corrector.remove(algorithm_constant.loc().double_colon.unwrap());
    corrector.remove(algorithm_constant.loc().name.unwrap());
    corrector.replace(
        correction_range(node),
        format!(
            "{}({})",
            node.range_source(node.loc().selector.unwrap()),
            replacement_args(node, algorithm_constant)
        ),
    );
}

fn message(node: Node<'_>, algorithm_constant: Node<'_>) -> String {
    let parent_constant = openssl_class(algorithm_constant);
    let replacement_args = replacement_args(node, algorithm_constant);
    let method = node.range_source(node.loc().selector.unwrap());
    format!(
        "Use `{parent_constant}.{method}({replacement_args})` instead of `{}`.",
        node.source()
    )
}

fn correction_range(node: Node<'_>) -> Range {
    range_between(node.loc().dot.unwrap().end_pos, node.source_range().end_pos)
}

fn openssl_class(node: Node<'_>) -> &str {
    node.child(0).map(|c| c.source()).unwrap_or_default()
}

fn algorithm_name(node: Node<'_>) -> String {
    let name = node.range_source(node.loc().name.unwrap());
    if openssl_class(node) == "OpenSSL::Cipher" && !NO_ARG_ALGORITHM.contains(&name) {
        // `name.scan(/.{3}/).join('-')`
        let chars: Vec<char> = name.chars().collect();
        chars
            .chunks(3)
            .filter(|chunk| chunk.len() == 3)
            .map(|chunk| chunk.iter().collect::<String>())
            .collect::<Vec<_>>()
            .join("-")
    } else {
        name.to_owned()
    }
}

/// Ruby's `String#split('-')` (trailing empty fields are dropped).
fn ruby_split_dash(s: &str) -> Vec<String> {
    let mut parts: Vec<String> = s.split('-').map(str::to_owned).collect();
    while parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

fn sanitize_arguments(arguments: &[Node<'_>]) -> Vec<String> {
    arguments
        .iter()
        .flat_map(|arg| {
            let argument = if arg.is(NodeType::Str) {
                arg.str_content().unwrap_or_default()
            } else {
                arg.source()
            };
            let cleaned: String = argument.chars().filter(|c| *c != ':' && *c != '\'').collect();
            ruby_split_dash(&cleaned)
        })
        .collect()
}

fn replacement_args(node: Node<'_>, algorithm_constant: Node<'_>) -> String {
    if algorithm_constant.source() == "OpenSSL::Cipher::Cipher" {
        return node.first_argument().unwrap().source().to_owned();
    }
    let algorithm_name = algorithm_name(algorithm_constant);
    if openssl_class(algorithm_constant) == "OpenSSL::Cipher" {
        build_cipher_arguments(node, &algorithm_name, node.arguments().is_empty())
    } else {
        std::iter::once(format!("'{algorithm_name}'"))
            .chain(node.arguments().iter().map(|a| a.source().to_owned()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn build_cipher_arguments(node: Node<'_>, algorithm_name: &str, no_arguments: bool) -> String {
    let algorithm_parts = ruby_split_dash(&algorithm_name.to_lowercase());
    let size_and_mode: Vec<String> = sanitize_arguments(&node.arguments())
        .iter()
        .map(|s| s.to_lowercase())
        .collect();
    let first = algorithm_parts.first().cloned().unwrap_or_default();
    if NO_ARG_ALGORITHM.contains(&first.to_uppercase().as_str()) && no_arguments {
        format!("'{first}'")
    } else {
        let mode = size_and_mode.is_empty().then(|| "cbc".to_owned());
        let parts: Vec<String> = algorithm_parts
            .into_iter()
            .chain(size_and_mode)
            .chain(mode)
            .take(3)
            .collect();
        format!("'{}'", parts.join("-"))
    }
}
