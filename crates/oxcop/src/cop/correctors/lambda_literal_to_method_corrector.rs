//! Port of `lib/rubocop/cop/correctors/lambda_literal_to_method_corrector.rb`:
//! rewrites `-> (x) { }` to `lambda { |x| }`.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::processed_source::Range;

pub struct LambdaLiteralToMethodCorrector<'a> {
    block_node: Node<'a>,
    method: Node<'a>,
    /// `block_node.arguments`: `None` stands for the empty array of
    /// numbered and `it` blocks.
    arguments: Option<Node<'a>>,
    source: &'a str,
}

impl<'a> LambdaLiteralToMethodCorrector<'a> {
    pub fn new(block_node: Node<'a>, source: &'a str) -> Self {
        Self {
            block_node,
            method: block_node.send_node().unwrap(),
            arguments: block_node
                .is(NodeType::Block)
                .then(|| block_node.child(1))
                .flatten(),
            source,
        }
    }

    pub fn call(&self, corrector: &mut Corrector) {
        // Check for unparenthesized args' preceding and trailing whitespaces.
        self.remove_unparenthesized_whitespace(corrector);

        if self.block_node.is(NodeType::Block) {
            // Avoid correcting to `lambdado` by inserting whitespace if none
            // exists before or after the lambda arguments.
            self.insert_separating_space(corrector);
            self.remove_arguments(corrector);
        }

        self.replace_selector(corrector);
        self.replace_delimiters(corrector);
        self.insert_arguments(corrector);
    }

    fn arguments_empty(&self) -> bool {
        self.arguments.is_none_or(|a| a.children().is_empty())
    }

    fn remove_unparenthesized_whitespace(&self, corrector: &mut Corrector) {
        if self.arguments_empty() || self.arguments.unwrap().is_parenthesized_call() {
            return;
        }
        self.remove_leading_whitespace(corrector);
        self.remove_trailing_whitespace(corrector);
    }

    fn insert_separating_space(&self, corrector: &mut Corrector) {
        if !self.needs_separating_space() {
            return;
        }
        corrector.insert_before(self.block_begin(), " ");
    }

    fn replace_selector(&self, corrector: &mut Corrector) {
        corrector.replace(self.method.source_range(), "lambda");
    }

    fn remove_arguments(&self, corrector: &mut Corrector) {
        let Some(arguments) = self.arguments else {
            return;
        };
        // `empty_and_without_delimiters?`
        if arguments.loc().expression.is_none() {
            return;
        }
        corrector.remove(arguments.source_range());
    }

    fn insert_arguments(&self, corrector: &mut Corrector) {
        if self.arguments_empty() {
            return;
        }
        let arg_str = format!(" |{}|", self.lambda_arg_string());
        corrector.insert_after(self.block_begin(), arg_str);
    }

    fn remove_leading_whitespace(&self, corrector: &mut Corrector) {
        let arguments = self.arguments.unwrap().source_range();
        corrector.remove_preceding(
            arguments,
            arguments.begin_pos - self.method.source_range().end_pos,
        );
    }

    fn remove_trailing_whitespace(&self, corrector: &mut Corrector) {
        let arguments = self.arguments.unwrap().source_range();
        let size = self.block_begin().begin_pos as isize - arguments.end_pos as isize - 1;
        if size > 0 {
            corrector.remove_preceding(self.block_begin(), size as usize);
        }
    }

    fn replace_delimiters(&self, corrector: &mut Corrector) {
        if self.block_node.has_braces() || !self.arg_to_unparenthesized_call() {
            return;
        }
        if !self.separating_space() {
            corrector.insert_after(self.block_begin(), " ");
        }
        corrector.replace(self.block_begin(), "{");
        corrector.replace(self.block_end(), "}");
    }

    fn lambda_arg_string(&self) -> String {
        // Block-local (shadow) arguments are separated from regular arguments
        // by a `;`; joining everything with `,` would turn them into extra
        // parameters and change the lambda's arity.
        let (shadow, regular): (Vec<Node<'_>>, Vec<Node<'_>>) = self
            .arguments
            .unwrap()
            .child_nodes()
            .partition(|arg| arg.is(NodeType::Shadowarg));
        let mut arg_string = regular
            .iter()
            .map(|a| a.source())
            .collect::<Vec<_>>()
            .join(", ");
        if !shadow.is_empty() {
            arg_string.push_str("; ");
            arg_string.push_str(
                &shadow
                    .iter()
                    .map(|a| a.source())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        arg_string
    }

    fn needs_separating_space(&self) -> bool {
        let selector_end = self.selector_end().end_pos;
        (Some(self.block_begin().begin_pos) == self.arguments_end_pos()
            && Some(selector_end) == self.arguments_begin_pos())
            || self.block_begin().begin_pos == selector_end
    }

    fn arguments_end_pos(&self) -> Option<usize> {
        self.arguments.and_then(|a| a.loc().end).map(|r| r.end_pos)
    }

    fn arguments_begin_pos(&self) -> Option<usize> {
        self.arguments
            .and_then(|a| a.loc().begin)
            .map(|r| r.begin_pos)
    }

    fn block_end(&self) -> Range {
        self.block_node.loc().end.unwrap()
    }

    fn block_begin(&self) -> Range {
        self.block_node.loc().begin.unwrap()
    }

    fn selector_end(&self) -> Range {
        self.method.loc().selector.unwrap().end()
    }

    fn arg_to_unparenthesized_call(&self) -> bool {
        let mut current_node = self.block_node;
        let mut parent = current_node.parent();
        if let Some(p) = parent.filter(|p| p.is(NodeType::Pair)) {
            let Some(hash) = p.parent() else {
                return false;
            };
            current_node = hash;
            parent = current_node.parent();
        }
        let Some(parent) = parent.filter(|p| p.is(NodeType::Send)) else {
            return false;
        };
        if parent.is_parenthesized_call() {
            return false;
        }
        current_node.sibling_index().is_some_and(|i| i > 1)
    }

    fn separating_space(&self) -> bool {
        self.source[self.block_begin().begin_pos + 2..]
            .chars()
            .next()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r'))
    }
}
