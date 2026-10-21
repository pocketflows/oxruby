//! Port of `lib/rubocop/cop/style/eval_with_location.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG_EVAL: &str = "Pass a binding, `__FILE__`, and `__LINE__` to `eval`.";

static VALID_EVAL_RECEIVER: NodePattern = NodePattern::new("{ nil? (const {nil? cbase} :Kernel) }");

static LINE_WITH_OFFSET: NodePattern = NodePattern::new(
    "{
       (send #special_line_keyword? %1 (int %2))
       (send (int %2) %1 #special_line_keyword?)
     }",
);

#[derive(Default)]
pub struct EvalWithLocation;

impl Cop for EvalWithLocation {
    fn name(&self) -> &'static str {
        "Style/EvalWithLocation"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["eval", "class_eval", "module_eval", "instance_eval"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.is_method("eval") {
            let valid = match node.receiver() {
                None => true,
                Some(receiver) => VALID_EVAL_RECEIVER.is_match(receiver),
            };
            if !valid {
                return;
            }
        }
        let Some(code) = node
            .first_argument()
            .filter(|c| matches!(c.ty(), NodeType::Str | NodeType::Dstr))
        else {
            return;
        };
        check_location(cx, node, code);
    }
}

fn check_location(cx: &mut Investigation<'_, '_>, node: Node<'_>, code: Node<'_>) {
    let base = if node.is_method("eval") { 2 } else { 1 };
    let arguments = node.arguments();
    let file = arguments.get(base).copied();
    let line = arguments.get(base + 1).copied();
    if line.is_some() {
        check_file(cx, node, file.unwrap());
        check_line(cx, node, code);
    } else if let Some(file) = file {
        check_file(cx, node, file);
        add_offense_for_missing_line(cx, node, code);
    } else {
        add_offense_for_missing_location(cx, node, code);
    }
}

fn message(node: Node<'_>) -> String {
    if node.is_method("eval") {
        MSG_EVAL.to_owned()
    } else {
        format!(
            "Pass `__FILE__` and `__LINE__` to `{}`.",
            node.method_name().unwrap_or("")
        )
    }
}

fn special_file_keyword(node: Node<'_>) -> bool {
    node.is(NodeType::Str) && node.source() == "__FILE__"
}

fn special_line_keyword(node: Node<'_>) -> bool {
    node.is(NodeType::Int) && node.source() == "__LINE__"
}

fn add_offense_for_incorrect_line(
    cx: &mut Investigation<'_, '_>,
    method_name: &str,
    line_node: Node<'_>,
    sign: &str,
    line_diff: i64,
) {
    let expected = expected_line(sign, line_diff);
    let message = format!(
        "Incorrect line number for `{method_name}`; use `{expected}` instead of `{}`.",
        line_node.source()
    );
    cx.add_offense_with_correction(line_node.source_range(), &message, |corrector| {
        corrector.replace(line_node.source_range(), expected);
    });
}

fn check_file(cx: &mut Investigation<'_, '_>, node: Node<'_>, file_node: Node<'_>) {
    if special_file_keyword(file_node) {
        return;
    }
    let message = format!(
        "Incorrect file for `{}`; use `__FILE__` instead of `{}`.",
        node.method_name().unwrap_or(""),
        file_node.source()
    );
    cx.add_offense_with_correction(file_node.source_range(), &message, |corrector| {
        corrector.replace(file_node.source_range(), "__FILE__");
    });
}

fn check_line(cx: &mut Investigation<'_, '_>, node: Node<'_>, code: Node<'_>) {
    let line_node = node.last_argument().unwrap();
    if line_node.is_variable() || (line_node.is_send() && !line_node.is_method("+")) {
        return;
    }
    let line_diff = line_difference(line_node, code);
    let method_name = node.method_name().unwrap_or("");
    if line_diff == 0 {
        if special_line_keyword(line_node) {
            return;
        }
        add_offense_for_incorrect_line(cx, method_name, line_node, "", 0);
    } else {
        let sign = if line_diff > 0 { "+" } else { "-" };
        if line_with_offset(line_node, sign, line_diff.abs()) {
            return;
        }
        add_offense_for_incorrect_line(cx, method_name, line_node, sign, line_diff.abs());
    }
}

fn line_with_offset(line_node: Node<'_>, sign: &str, diff: i64) -> bool {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "special_line_keyword?" && val.node().is_some_and(special_line_keyword)
    };
    let params = [Param::Sym(sign), Param::Int(diff)];
    let ctx = MatchContext {
        params: &params,
        call: Some(&call),
        ..Default::default()
    };
    LINE_WITH_OFFSET.match_with(line_node, &ctx).is_some()
}

fn line_difference(line_node: Node<'_>, code: Node<'_>) -> i64 {
    string_first_line(code) as i64 - line_node.first_line() as i64
}

fn string_first_line(str_node: Node<'_>) -> usize {
    if str_node.is_heredoc() {
        str_node.line_of(str_node.loc().heredoc_body.unwrap())
    } else {
        str_node.first_line()
    }
}

fn expected_line(sign: &str, line_diff: i64) -> String {
    if line_diff == 0 {
        "__LINE__".to_owned()
    } else {
        format!("__LINE__ {sign} {}", line_diff.abs())
    }
}

fn add_offense_for_missing_line(cx: &mut Investigation<'_, '_>, node: Node<'_>, code: Node<'_>) {
    let line_str = missing_line(node, code);
    let end = node.last_argument().unwrap().source_range().end();
    cx.add_offense_with_correction(node.source_range(), &message(node), |corrector| {
        corrector.insert_after(end, format!(", {line_str}"));
    });
}

fn add_offense_for_missing_location(cx: &mut Investigation<'_, '_>, node: Node<'_>, code: Node<'_>) {
    if node.is_method("eval") && node.arguments().len() < 2 {
        cx.add_offense(node.source_range(), &message(node));
        return;
    }
    let line_str = missing_line(node, code);
    let end = node.last_argument().unwrap().source_range().end();
    cx.add_offense_with_correction(node.source_range(), &message(node), |corrector| {
        corrector.insert_after(end, format!(", __FILE__, {line_str}"));
    });
}

fn missing_line(node: Node<'_>, code: Node<'_>) -> String {
    let line_diff = line_difference(node.last_argument().unwrap(), code);
    let sign = if line_diff > 0 { "+" } else { "-" };
    expected_line(sign, line_diff)
}
