//! Port of `lib/rubocop/cop/style/single_line_block_params.rb`.

use serde_yaml_ng::Value;

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct SingleLineBlockParams;

impl Cop for SingleLineBlockParams {
    fn name(&self) -> &'static str {
        "Style/SingleLineBlockParams"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !node.is_single_line() {
            return;
        }
        let methods = methods(cx);
        let method_name = node.method_name().unwrap_or_default();
        if node.receiver().is_none() || !methods.iter().any(|(name, _)| name == method_name) {
            return;
        }
        let args = node.arguments();
        if args.is_empty() || !args.iter().all(|a| a.is(NodeType::Arg)) {
            return;
        }
        let target_args = target_args(&methods, method_name);
        if args_match(&target_args, &args) {
            return;
        }
        // `build_preferred_arguments_map`: a Hash keyed by the current names.
        let mut preferred: Vec<(String, String)> = Vec::new();
        for (index, current_lvar) in args.iter().enumerate() {
            let mut preferred_argument = target_args.get(index).cloned().unwrap_or_default();
            let current_argument = current_lvar.source().to_owned();
            if current_argument.starts_with('_') {
                preferred_argument = format!("_{preferred_argument}");
            }
            match preferred.iter_mut().find(|(k, _)| *k == current_argument) {
                Some(entry) => entry.1 = preferred_argument,
                None => preferred.push((current_argument, preferred_argument)),
            }
        }
        let joined = preferred
            .iter()
            .map(|(_, v)| v.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!("Name `{method_name}` block params `|{joined}|`.");
        let arguments = node.arguments_node().unwrap();
        cx.add_offense_with_correction(arguments.source_range(), &message, |corrector| {
            corrector.replace(arguments.source_range(), format!("|{joined}|"));
            for lvar in node.descendants().filter(|d| d.is(NodeType::Lvar)) {
                if let Some((_, preferred_lvar)) = preferred.iter().find(|(k, _)| k == lvar.source())
                {
                    corrector.replace(lvar.source_range(), preferred_lvar);
                }
            }
        });
    }
}

/// `cop_config['Methods']`: `[{reduce: [acc, elem]}, ...]`.
fn methods(cx: &Investigation<'_, '_>) -> Vec<(String, Vec<String>)> {
    let Some(Value::Sequence(items)) = cx.cop_config_value("Methods") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let (key, value) = item.as_mapping()?.iter().next()?;
            let args = match value {
                Value::Sequence(args) => args
                    .iter()
                    .filter_map(|a| a.as_str().map(str::to_owned))
                    .collect(),
                _ => Vec::new(),
            };
            Some((key.as_str()?.to_owned(), args))
        })
        .collect()
}

fn target_args(methods: &[(String, Vec<String>)], method_name: &str) -> Vec<String> {
    methods
        .iter()
        .find(|(name, _)| name == method_name)
        .map(|(_, args)| args.clone())
        .unwrap_or_default()
}

fn args_match(target_args: &[String], args: &[Node<'_>]) -> bool {
    let actual: Vec<String> = args
        .iter()
        .filter_map(|a| a.name())
        .map(|name| name.trim_start_matches('_').to_owned())
        .collect();
    let expected: Vec<String> = target_args.iter().take(actual.len()).cloned().collect();
    actual == expected
}
