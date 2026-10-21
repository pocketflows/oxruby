//! Port of `lib/rubocop/cop/lint/erb_new_arguments.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::arguments_range;
use crate::cop::{Cop, Corrector, Investigation};

const MESSAGE_SAFE_LEVEL: &str = "Passing safe_level with the 2nd argument of `ERB.new` is deprecated. Do not use it, and specify other arguments as keyword arguments.";

static ERB_NEW_WITH_NON_KEYWORD_ARGUMENTS: NodePattern =
    NodePattern::new("(send (const {nil? cbase} :ERB) :new $...)");

#[derive(Default)]
pub struct ErbNewArguments;

impl Cop for ErbNewArguments {
    fn name(&self) -> &'static str {
        "Lint/ErbNewArguments"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.6)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["new"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = ERB_NEW_WITH_NON_KEYWORD_ARGUMENTS.match_node(node) else {
            return;
        };
        let arguments = captures[0].nodes();
        if arguments.is_empty() || correct_arguments(&arguments) {
            return;
        }
        for (i, argument) in arguments.iter().skip(1).take(3).enumerate() {
            if argument.is(NodeType::Hash) {
                continue;
            }
            let message = message(i, argument.source());
            cx.add_offense_with_correction(argument.source_range(), &message, |corrector| {
                autocorrect(corrector, node);
            });
        }
    }
}

fn message(positional_argument_index: usize, arg_value: &str) -> String {
    match positional_argument_index {
        0 => MESSAGE_SAFE_LEVEL.to_owned(),
        1 => format!(
            "Passing trim_mode with the 3rd argument of `ERB.new` is deprecated. Use keyword argument like `ERB.new(str, trim_mode: {arg_value})` instead."
        ),
        _ => format!(
            "Passing eoutvar with the 4th argument of `ERB.new` is deprecated. Use keyword argument like `ERB.new(str, eoutvar: {arg_value})` instead."
        ),
    }
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    let str_arg = node.first_argument().unwrap().source().to_owned();
    let kwargs = build_kwargs(node);
    let overridden_kwargs = override_by_legacy_args(kwargs, node);
    let good_arguments: Vec<String> = std::iter::once(Some(str_arg))
        .chain(overridden_kwargs)
        .flatten()
        .collect();
    corrector.replace(arguments_range(node), good_arguments.join(", "));
}

fn correct_arguments(arguments: &[Node<'_>]) -> bool {
    arguments.len() == 1 || (arguments.len() == 2 && arguments[1].is(NodeType::Hash))
}

fn build_kwargs(node: Node<'_>) -> [Option<String>; 2] {
    let last_argument = node.last_argument().unwrap();
    if !last_argument.is(NodeType::Hash) {
        return [None, None];
    }
    let mut trim_mode_arg = None;
    let mut eoutvar_arg = None;
    for pair in last_argument.pairs() {
        let (Some(key), Some(value)) = (pair.key(), pair.pair_value()) else {
            continue;
        };
        match key.source() {
            "trim_mode" => trim_mode_arg = Some(format!("trim_mode: {}", value.source())),
            "eoutvar" => eoutvar_arg = Some(format!("eoutvar: {}", value.source())),
            _ => {}
        }
    }
    [trim_mode_arg, eoutvar_arg]
}

fn override_by_legacy_args(kwargs: [Option<String>; 2], node: Node<'_>) -> [Option<String>; 2] {
    let arguments = node.arguments();
    let mut overridden_kwargs = kwargs;
    if let Some(argument) = arguments.get(2).filter(|a| !a.is(NodeType::Hash)) {
        overridden_kwargs[0] = Some(format!("trim_mode: {}", argument.source()));
    }
    if let Some(argument) = arguments.get(3).filter(|a| !a.is(NodeType::Hash)) {
        overridden_kwargs[1] = Some(format!("eoutvar: {}", argument.source()));
    }
    overridden_kwargs
}
