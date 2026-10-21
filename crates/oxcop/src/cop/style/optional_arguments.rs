//! Port of `lib/rubocop/cop/style/optional_arguments.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Optional arguments should appear at the end of the argument list.";

#[derive(Default)]
pub struct OptionalArguments;

impl Cop for OptionalArguments {
    fn name(&self) -> &'static str {
        "Style/OptionalArguments"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let arguments = node.arguments();
        let optarg_positions: Vec<usize> = (0..arguments.len())
            .filter(|&i| arguments[i].is(NodeType::Optarg))
            .collect();
        let Some(&max_arg) = (0..arguments.len())
            .filter(|&i| arguments[i].is(NodeType::Arg))
            .collect::<Vec<_>>()
            .iter()
            .max()
        else {
            return;
        };
        for optarg_position in optarg_positions {
            // there can only be one group of optional arguments
            if optarg_position > max_arg {
                break;
            }
            cx.add_offense(arguments[optarg_position].source_range(), MSG);
        }
    }
}
