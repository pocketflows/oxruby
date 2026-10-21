//! Port of `lib/rubocop/cop/style/trailing_comma_in_block_args.rb`.

use crate::ast::lexer::Token;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;

const MSG: &str = "Useless trailing comma present in block arguments.";

#[derive(Default)]
pub struct TrailingCommaInBlockArgs;

impl Cop for TrailingCommaInBlockArgs {
    fn name(&self) -> &'static str {
        "Style/TrailingCommaInBlockArgs"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.send_node().is_some_and(|s| s.is_lambda_literal()) {
            return;
        }
        if arg_count(node) <= 1 {
            return;
        }
        let Some(last) = argument_tokens(cx.processed_source, node).and_then(|t| t.last()) else {
            return;
        };
        if last.ty != "tCOMMA" {
            return;
        }
        let range = last.range;
        cx.add_offense_with_correction(range, MSG, |corrector| {
            corrector.replace(range, "");
        });
    }
}

fn arg_count(node: Node<'_>) -> usize {
    node.arguments_node().map_or(0, |args| {
        args.descendants()
            .filter(|d| d.is_any(&[NodeType::Arg, NodeType::Optarg, NodeType::Kwoptarg]))
            .count()
    })
}

fn argument_tokens<'p>(ps: &'p ProcessedSource<'_>, node: Node<'_>) -> Option<&'p [Token]> {
    let tokens = ps.tokens_within(node.source_range());
    let mut pipes = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.ty == "tPIPE")
        .map(|(i, _)| i);
    let begin = pipes.next()?;
    let end = pipes.next()?;
    tokens.get(begin + 1..end)
}
