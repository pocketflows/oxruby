//! Port of `lib/rubocop/cop/lint/format_parameter_mismatch.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::utils::format_string::FormatString;
use crate::cop::{Cop, Investigation};

const MSG_INVALID: &str = "Format string is invalid because formatting sequence types (numbered, named or unnumbered) are mixed.";
const KERNEL: &str = "Kernel";
const SHOVEL: &str = "<<";

static CALLED_ON_STRING: NodePattern = NodePattern::new(
    "{(send {nil? const_type?} _ {str dstr} ...)
      (send {str dstr} ...)}",
);

#[derive(Default)]
pub struct FormatParameterMismatch;

impl Cop for FormatParameterMismatch {
    fn name(&self) -> &'static str {
        "Lint/FormatParameterMismatch"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["format", "sprintf", "%"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !format_string(node) {
            return;
        }
        let selector = node.loc().selector.unwrap();
        if invalid_format_string(node) {
            cx.add_offense(selector, MSG_INVALID);
            return;
        }
        if !offending_node(node) {
            return;
        }
        cx.add_offense(selector, &message(node));
    }
}

/// `count_matches`: `None` is `:unknown`.
type Counts = Option<(i64, i64)>;

fn format_string(node: Node<'_>) -> bool {
    CALLED_ON_STRING.is_match(node) && method_with_format_args(node)
}

fn invalid_format_string(node: Node<'_>) -> bool {
    let string = if sprintf(node) || format(node) {
        node.first_argument().unwrap().source()
    } else {
        node.receiver().unwrap().source()
    };
    !FormatString::new(string).is_valid()
}

fn offending_node(node: Node<'_>) -> bool {
    if splat_args(node) {
        return false;
    }
    let Some((num_of_format_args, num_of_expected_fields)) = count_matches(node) else {
        return false;
    };
    let first_arg = node.first_argument().unwrap();
    if num_of_expected_fields == 0 && first_arg.is_any(&[NodeType::Dstr, NodeType::Array]) {
        return false;
    }
    matched_arguments_count(num_of_expected_fields, num_of_format_args)
}

fn matched_arguments_count(expected: i64, passed: i64) -> bool {
    if passed < 0 {
        expected < passed.abs()
    } else {
        expected != passed
    }
}

fn method_with_format_args(node: Node<'_>) -> bool {
    sprintf(node) || format(node) || percent(node)
}

fn splat_args(node: Node<'_>) -> bool {
    if percent(node) {
        return false;
    }
    node.arguments()
        .iter()
        .skip(1)
        .any(|a| a.is(NodeType::Splat))
}

fn heredoc(node: Node<'_>) -> bool {
    node.first_argument()
        .is_some_and(|a| a.source().starts_with(SHOVEL))
}

fn count_matches(node: Node<'_>) -> Counts {
    if countable_format(node) {
        count_format_matches(node)
    } else if countable_percent(node) {
        count_percent_matches(node)
    } else {
        None
    }
}

fn countable_format(node: Node<'_>) -> bool {
    (sprintf(node) || format(node)) && !heredoc(node)
}

fn countable_percent(node: Node<'_>) -> bool {
    percent(node) && node.first_argument().is_some_and(|a| a.is(NodeType::Array))
}

fn count_format_matches(node: Node<'_>) -> Counts {
    let expected = expected_fields_count(node.first_argument().unwrap())?;
    Some((node.arguments().len() as i64 - 1, expected))
}

fn count_percent_matches(node: Node<'_>) -> Counts {
    let expected = expected_fields_count(node.receiver().unwrap())?;
    Some((
        node.first_argument().unwrap().child_nodes().count() as i64,
        expected,
    ))
}

fn format_method(name: &str, node: Node<'_>) -> bool {
    if node.is_const_receiver()
        && !node
            .receiver()
            .and_then(|r| r.loc().name.map(|n| r.range_source(n)))
            .is_some_and(|n| n == KERNEL)
    {
        return false;
    }
    if !node.is_method(name) {
        return false;
    }
    node.arguments().len() > 1 && string_type(node.first_argument().unwrap())
}

fn expected_fields_count(node: Node<'_>) -> Option<i64> {
    if !string_type(node) {
        return None;
    }
    let format_string = FormatString::new(node.source());
    if format_string.is_named_interpolation() {
        return Some(1);
    }
    if let Some(max_digit_dollar_num) = format_string.max_digit_dollar_num()
        && max_digit_dollar_num != 0
    {
        return Some(max_digit_dollar_num as i64);
    }
    Some(
        format_string
            .format_sequences()
            .iter()
            .filter(|seq| !seq.is_percent())
            .map(|seq| seq.arity() as i64)
            .sum(),
    )
}

fn format(node: Node<'_>) -> bool {
    format_method("format", node)
}

fn sprintf(node: Node<'_>) -> bool {
    format_method("sprintf", node)
}

fn percent(node: Node<'_>) -> bool {
    let receiver = node.receiver();
    let percent = node.is_method("%")
        && (receiver.is_some_and(string_type)
            || node.first_argument().is_some_and(|a| a.is(NodeType::Array)));
    if percent && receiver.is_some_and(string_type) && heredoc(node) {
        return false;
    }
    percent
}

fn message(node: Node<'_>) -> String {
    let (num_args_for_format, num_expected_fields) = count_matches(node).unwrap_or_default();
    let method_name = if node.is_method("%") {
        "String#%"
    } else {
        node.method_name().unwrap()
    };
    format!(
        "Number of arguments ({num_args_for_format}) to `{method_name}` doesn't match the number of fields ({num_expected_fields})."
    )
}

fn string_type(node: Node<'_>) -> bool {
    node.is_any(&[NodeType::Str, NodeType::Dstr])
}
