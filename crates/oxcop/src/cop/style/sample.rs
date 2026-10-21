//! Port of `lib/rubocop/cop/style/sample.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

static SAMPLE_CANDIDATE: NodePattern =
    NodePattern::new("(call $(call _ :shuffle $...) ${:first :last :[] :at :slice} $...)");

/// `sample_size`: `nil`, `:unknown` or an integer.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SampleSize {
    Nil,
    Unknown,
    Size(i64),
}

#[derive(Default)]
pub struct Sample;

impl Cop for Sample {
    fn name(&self) -> &'static str {
        "Style/Sample"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["first", "last", "[]", "at", "slice"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = SAMPLE_CANDIDATE.match_node(node) else {
            return;
        };
        let shuffle_node = captures[0].node().unwrap();
        let shuffle_arg = captures[1].nodes();
        let method = captures[2].sym().unwrap_or_default();
        let method_args = captures[3].nodes();
        if !offensive(method, &method_args) {
            return;
        }
        let range = shuffle_node
            .loc()
            .selector
            .unwrap()
            .join(node.source_range().end());
        let correct = correction(&shuffle_arg, method, &method_args);
        let message = format!("Use `{correct}` instead of `{}`.", cx.source(range));
        if shuffle_arg.is_empty() {
            cx.add_offense_with_correction(range, &message, |corrector| {
                corrector.replace(range, correct);
            });
        } else {
            cx.add_offense(range, &message);
        }
    }
}

fn offensive(method: &str, method_args: &[Node<'_>]) -> bool {
    match method {
        "first" | "last" => true,
        "[]" | "at" | "slice" => sample_size(method_args) != SampleSize::Unknown,
        _ => false,
    }
}

fn sample_size(method_args: &[Node<'_>]) -> SampleSize {
    match method_args {
        [arg] => sample_size_for_one_arg(*arg),
        [first, second] => sample_size_for_two_args(*first, *second),
        _ => SampleSize::Nil,
    }
}

fn sample_size_for_one_arg(arg: Node<'_>) -> SampleSize {
    if arg.is_range() {
        range_size(arg)
    } else if let Some(value) = arg.int_value() {
        if value == 0 || value == -1 {
            SampleSize::Nil
        } else {
            SampleSize::Unknown
        }
    } else {
        SampleSize::Unknown
    }
}

fn sample_size_for_two_args(first: Node<'_>, second: Node<'_>) -> SampleSize {
    if first.int_value() != Some(0) {
        return SampleSize::Unknown;
    }
    match second.int_value() {
        Some(value) => SampleSize::Size(value),
        None => SampleSize::Unknown,
    }
}

fn range_size(range_node: Node<'_>) -> SampleSize {
    let mut vals = Vec::new();
    for child in range_node.children() {
        match child.as_node() {
            None => vals.push(0),
            Some(id) => match range_node.ast().node(id).int_value() {
                Some(value) => vals.push(value),
                None => return SampleSize::Unknown,
            },
        }
    }
    let [low, high] = vals[..] else {
        return SampleSize::Unknown;
    };
    if low != 0 || high < 0 {
        return SampleSize::Unknown;
    }
    if range_node.is(NodeType::Erange) {
        SampleSize::Size(high - low)
    } else {
        SampleSize::Size(high - low + 1)
    }
}

fn correction(shuffle_arg: &[Node<'_>], method: &str, method_args: &[Node<'_>]) -> String {
    let shuffle_arg = extract_source(shuffle_arg);
    let sample_arg = sample_arg(method, method_args);
    let args: Vec<String> = [sample_arg, shuffle_arg].into_iter().flatten().collect();
    if args.is_empty() {
        "sample".to_owned()
    } else {
        format!("sample({})", args.join(", "))
    }
}

fn sample_arg(method: &str, method_args: &[Node<'_>]) -> Option<String> {
    match method {
        "first" | "last" => extract_source(method_args),
        "[]" | "slice" => match sample_size(method_args) {
            SampleSize::Size(size) => Some(size.to_string()),
            SampleSize::Unknown => Some("unknown".to_owned()),
            SampleSize::Nil => None,
        },
        _ => None,
    }
}

fn extract_source(args: &[Node<'_>]) -> Option<String> {
    args.first().map(|a| a.source().to_owned())
}
