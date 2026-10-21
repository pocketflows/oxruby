//! Port of `lib/rubocop/cop/correctors/if_then_corrector.rb`: rewrites
//! `if cond then body` to the multi-line form.

use crate::ast::{Node, NodeType};
use crate::cop::Corrector;

const DEFAULT_INDENTATION_WIDTH: usize = 2;

pub struct IfThenCorrector<'a> {
    if_node: Node<'a>,
    indentation: usize,
}

impl<'a> IfThenCorrector<'a> {
    pub fn new(if_node: Node<'a>, indentation: Option<usize>) -> Self {
        Self {
            if_node,
            indentation: indentation.unwrap_or(DEFAULT_INDENTATION_WIDTH),
        }
    }

    pub fn call(&self, corrector: &mut Corrector) {
        corrector.replace(
            self.if_node.source_range(),
            self.replacement(self.if_node, None),
        );
    }

    fn replacement(&self, node: Node<'_>, indentation: Option<String>) -> String {
        let indentation = indentation.unwrap_or_else(|| " ".repeat(node.column()));
        let if_branch_source = node.if_branch().map_or("nil", |b| b.source());
        let elsif_indentation = if node.is_elsif() {
            indentation.as_str()
        } else {
            ""
        };
        let if_branch = format!(
            "{elsif_indentation}{} {}\n{indentation}{}{if_branch_source}\n",
            node.keyword(),
            node.condition().unwrap().source(),
            self.branch_body_indentation()
        );
        let else_branch = self.rewrite_else_branch(node.else_branch(), &indentation);
        if_branch + &else_branch
    }

    fn rewrite_else_branch(&self, else_branch: Option<Node<'_>>, indentation: &str) -> String {
        match else_branch {
            None => "end".to_owned(),
            Some(e) if e.is(NodeType::If) && e.is_elsif() => {
                self.replacement(e, Some(indentation.to_owned()))
            }
            Some(e) => format!(
                "{indentation}else\n{indentation}{}{}\n{indentation}end",
                self.branch_body_indentation(),
                e.source()
            ),
        }
    }

    fn branch_body_indentation(&self) -> String {
        " ".repeat(self.indentation)
    }
}
