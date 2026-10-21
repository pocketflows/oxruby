//! Port of `lib/rubocop/cop/style/infinite_loop.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::alignment::indentation;
use crate::cop::variable_force::{Scope, VariableTable};
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Use `Kernel#loop` for infinite loops.";

/// What `@variables` keeps of a variable: its assignment and reference
/// ranges.
struct VariableData {
    assignments: Vec<Range>,
    references: Vec<Range>,
}

#[derive(Default)]
pub struct InfiniteLoop {
    variables: Vec<VariableData>,
}

impl Cop for InfiniteLoop {
    fn name(&self) -> &'static str {
        "Style/InfiniteLoop"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn joins_variable_force(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::While,
            NodeType::Until,
            NodeType::WhilePost,
            NodeType::UntilPost,
        ]
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.variables.clear();
    }

    fn after_leaving_scope<'a>(
        &mut self,
        _cx: &mut Investigation<'a, '_>,
        scope: &Scope<'a>,
        variable_table: &VariableTable<'a>,
    ) {
        for variable in variable_table.scope_variables_of(scope) {
            self.variables.push(VariableData {
                assignments: variable
                    .assignments
                    .iter()
                    .map(|a| a.node.source_range())
                    .collect(),
                references: variable
                    .references
                    .iter()
                    .map(|r| r.node.source_range())
                    .collect(),
            });
        }
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(condition) = node.condition() else {
            return;
        };
        let infinite = if node.is_any(&[NodeType::While, NodeType::WhilePost]) {
            condition.is_truthy_literal()
        } else {
            condition.is_falsey_literal()
        };
        if infinite {
            self.while_or_until(cx, node);
        }
    }
}

impl InfiniteLoop {
    fn while_or_until(&self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        let range = node.source_range();
        // Not every `while true` and `until false` can be turned into a
        // `loop do` without further modification: a variable introduced
        // inside the loop is in scope after it, but not after a block.
        if self.variables.iter().any(|var| {
            assigned_inside_loop(var, range)
                && !assigned_before_loop(var, range)
                && referenced_after_loop(var, range)
        }) {
            return;
        }
        cx.add_offense_cx(node.loc().keyword.unwrap(), MSG, |corrector, cx| {
            autocorrect(cx, corrector, node);
        });
    }
}

fn autocorrect(cx: &Investigation<'_, '_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.is_post_condition_loop() {
        let body = node.body().unwrap();
        corrector.replace(body.loc().begin.unwrap(), "loop do");
        corrector.remove(
            body.loc()
                .end
                .unwrap()
                .end()
                .join(node.source_range().end()),
        );
    } else if node.is_modifier_form() {
        corrector.replace(node.source_range(), modifier_replacement(cx, node));
    } else {
        corrector.replace(non_modifier_range(node), "loop do");
    }
}

fn assigned_inside_loop(var: &VariableData, range: Range) -> bool {
    var.assignments.iter().any(|a| range.contains(*a))
}

fn assigned_before_loop(var: &VariableData, range: Range) -> bool {
    var.assignments.iter().any(|a| a.end_pos < range.begin_pos)
}

fn referenced_after_loop(var: &VariableData, range: Range) -> bool {
    var.references.iter().any(|r| r.begin_pos > range.end_pos)
}

fn modifier_replacement(cx: &Investigation<'_, '_>, node: Node<'_>) -> String {
    let body = node.body().unwrap();
    if node.is_single_line() {
        return format!("loop {{ {} }}", body.source());
    }
    let ps = cx.processed_source;
    let line = ps.source_line(body.first_line());
    let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    let indent = indentation(cx, node);
    let indented_body: String = body
        .source()
        .split_inclusive('\n')
        .map(|l| format!("{indent}{l}"))
        .collect();
    [String::from("loop do"), indented_body, String::from("end")].join(&format!("\n{leading}"))
}

fn non_modifier_range(node: Node<'_>) -> Range {
    let start_range = node.loc().keyword.unwrap().begin();
    let end_range = if node.has_do() {
        node.loc().begin.unwrap().end()
    } else {
        node.condition().unwrap().source_range().end()
    };
    start_range.join(end_range)
}
