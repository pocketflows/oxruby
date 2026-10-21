//! Port of `lib/rubocop/cop/lint/unreachable_code.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Unreachable code detected.";

static FLOW_COMMAND: NodePattern = NodePattern::new(
    "{
       return next break retry redo
       (send
        {nil? (const {nil? cbase} :Kernel)}
        #redefinable_flow_method?
        ...)
     }",
);

#[derive(Default)]
pub struct UnreachableCode {
    redefined: Vec<String>,
    instance_eval_count: i64,
}

impl Cop for UnreachableCode {
    fn name(&self) -> &'static str {
        "Lint/UnreachableCode"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::Block,
            NodeType::Numblock,
            NodeType::Itblock,
            NodeType::Begin,
            NodeType::Kwbegin,
        ]
    }

    fn after_node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::Begin | NodeType::Kwbegin => self.on_begin(cx, node),
            _ => {
                if instance_eval_block(node) {
                    self.instance_eval_count += 1;
                }
            }
        }
    }

    fn after_node<'a>(&mut self, _cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if instance_eval_block(node) {
            self.instance_eval_count -= 1;
        }
    }
}

fn redefinable_flow_method(method: &str) -> bool {
    matches!(method, "raise" | "fail" | "throw" | "exit" | "exit!" | "abort")
}

fn flow_command(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _args: &[Val<'_>]| {
        name == "redefinable_flow_method?" && val.sym().is_some_and(redefinable_flow_method)
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    FLOW_COMMAND.match_with(node, &ctx).is_some()
}

fn instance_eval_block(node: Node<'_>) -> bool {
    node.is_any_block() && node.is_method("instance_eval")
}

impl UnreachableCode {
    fn on_begin(&mut self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        let expressions: Vec<Node<'_>> = node.child_nodes().collect();
        let mut flow_reached = false;
        for (index, expression) in expressions.iter().enumerate() {
            if flow_reached {
                cx.add_offense(expression.source_range(), MSG);
            } else if index < expressions.len() - 1 && self.flow_expression(*expression) {
                flow_reached = true;
            }
        }
    }

    fn flow_expression(&mut self, node: Node<'_>) -> bool {
        if flow_command(node) {
            return self.report_on_flow_command(node);
        }

        match node.ty() {
            NodeType::Begin | NodeType::Kwbegin => {
                let expressions: Vec<Node<'_>> = node.child_nodes().collect();
                expressions.into_iter().any(|expr| self.flow_expression(expr))
            }
            NodeType::If => self.check_if(node),
            NodeType::Case | NodeType::CaseMatch => self.check_case(node),
            NodeType::Def | NodeType::Defs => {
                self.register_redefinition(node);
                false
            }
            _ => false,
        }
    }

    fn check_if(&mut self, node: Node<'_>) -> bool {
        let (Some(if_branch), Some(else_branch)) = (node.if_branch(), node.else_branch()) else {
            return false;
        };
        self.flow_expression(if_branch) && self.flow_expression(else_branch)
    }

    fn check_case(&mut self, node: Node<'_>) -> bool {
        let Some(else_branch) = node.else_branch() else {
            return false;
        };
        if !self.flow_expression(else_branch) {
            return false;
        }

        node.when_branches()
            .into_iter()
            .all(|branch| branch.body().is_some_and(|body| self.flow_expression(body)))
    }

    fn register_redefinition(&mut self, node: Node<'_>) {
        if let Some(method_name) = node.method_name()
            && redefinable_flow_method(method_name)
        {
            self.redefined.push(method_name.to_owned());
        }
    }

    fn report_on_flow_command(&self, node: Node<'_>) -> bool {
        if !node.is(NodeType::Send) {
            return true;
        }
        if node.receiver().is_some() {
            return true;
        }
        if self.instance_eval_count > 0 {
            return false;
        }

        !node
            .method_name()
            .is_some_and(|m| self.redefined.iter().any(|r| r == m))
    }
}
