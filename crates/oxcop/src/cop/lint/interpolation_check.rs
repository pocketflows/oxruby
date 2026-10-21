//! Port of `lib/rubocop/cop/lint/interpolation_check.rb`.

use crate::ast::{MapKind, Node, NodeType};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::ProcessedSource;
use crate::regex;

const MSG: &str = "Interpolation in single quoted string detected. Use double quoted strings if you need interpolation.";

#[derive(Default)]
pub struct InterpolationCheck;

impl Cop for InterpolationCheck {
    fn name(&self) -> &'static str {
        "Lint/InterpolationCheck"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Str, NodeType::Dstr]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is(NodeType::Dstr) {
            if heredoc(node) {
                return;
            }
            if node.loc_is(node.loc().begin, "'") {
                check(cx, node);
            }
        } else {
            check(cx, node);
        }
    }
}

fn check<'a>(cx: &mut Investigation<'a, '_>, node: Node<'a>) {
    if node.parent().is_some_and(|p| p.is(NodeType::Regexp)) {
        return;
    }
    if regex!(r"(?<!\\)#\{.*\}").find(node.source()).is_none() {
        return;
    }
    if heredoc(node) {
        return;
    }
    if node.loc().begin.is_none() || node.loc().end.is_none() {
        return;
    }
    if !valid_syntax(cx, node) {
        return;
    }
    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
        autocorrect(corrector, node);
    });
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    let (starting_token, ending_token) = if node.source().contains('"') {
        ("%{", "}")
    } else {
        ("\"", "\"")
    };
    corrector.replace(node.loc().begin.unwrap(), starting_token);
    corrector.replace(node.loc().end.unwrap(), ending_token);
}

fn heredoc(node: Node<'_>) -> bool {
    node.loc().kind == MapKind::Heredoc || node.parent().is_some_and(heredoc)
}

fn valid_syntax(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let source = node.source();
    let double_quoted_string = if source.contains('"') {
        let s = regex!(r"\A'").replace(source, "%{");
        regex!(r"'\z").replace(&s, "}")
    } else {
        regex!(r"\A'|'\z").replace_all(source, "\"")
    };
    let processed_source = ProcessedSource::new(
        &double_quoted_string,
        &double_quoted_string,
        cx.target_ruby_version(),
        cx.processed_source.path(),
    );
    processed_source.valid_syntax()
        && processed_source
            .ast()
            .is_some_and(|ast| ast.is(NodeType::Dstr))
}
