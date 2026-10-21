//! Port of `lib/rubocop/cop/lint/to_enum_arguments.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Ensure you correctly provided all the arguments.";

static ENUM_CONVERSION_CALL: NodePattern =
    NodePattern::new("(send {nil? self} {:to_enum :enum_for} $_ $...)");
static METHOD_NAME: NodePattern =
    NodePattern::new("{(send nil? {:__method__ :__callee__}) (sym %1)}");
static PASSING_KEYWORD_ARG: NodePattern = NodePattern::new("(pair (sym %1) (lvar %1))");

#[derive(Default)]
pub struct ToEnumArguments;

impl Cop for ToEnumArguments {
    fn name(&self) -> &'static str {
        "Lint/ToEnumArguments"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["to_enum", "enum_for"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(def_node) = node.ancestors().find(|a| a.is_any_def()) else {
            return;
        };
        let Some(captures) = ENUM_CONVERSION_CALL.match_node(node) else {
            return;
        };
        let Some(method_node) = captures[0].node() else {
            return;
        };
        let arguments = captures[1].nodes();

        if !method_name(method_node, def_node.method_name().unwrap_or_default())
            || arguments_match(&arguments, def_node)
        {
            return;
        }

        cx.add_offense(node.source_range(), MSG);
    }
}

fn with_param<R>(name: &str, f: impl FnOnce(&MatchContext<'_, '_>) -> R) -> R {
    let params = [Param::Sym(name)];
    let ctx = MatchContext {
        params: &params,
        ..Default::default()
    };
    f(&ctx)
}

fn method_name(node: Node<'_>, name: &str) -> bool {
    with_param(name, |ctx| METHOD_NAME.match_with(node, ctx).is_some())
}

fn passing_keyword_arg(node: Node<'_>, name: &str) -> bool {
    with_param(name, |ctx| PASSING_KEYWORD_ARG.match_with(node, ctx).is_some())
}

fn arguments_match(arguments: &[Node<'_>], def_node: Node<'_>) -> bool {
    let mut index = 0;
    let all_present = def_node
        .arguments()
        .into_iter()
        .filter(|a| !a.is(NodeType::Blockarg))
        .all(|def_arg| {
            let send_arg = arguments.get(index).copied();
            if matches!(
                def_arg.ty(),
                NodeType::Arg | NodeType::Restarg | NodeType::Optarg
            ) {
                index += 1;
            }
            send_arg.is_some_and(|send_arg| argument_match(send_arg, def_arg))
        });

    all_present
        && !extra_positional_arguments(arguments, def_node)
        && !extra_keyword_arguments(arguments, def_node)
}

fn extra_positional_arguments(arguments: &[Node<'_>], def_node: Node<'_>) -> bool {
    if variadic_parameters(def_node) || expandable_arguments(arguments) {
        return false;
    }

    positional_arguments(arguments) > positional_parameters(def_node)
}

fn variadic_parameters(def_node: Node<'_>) -> bool {
    def_node
        .arguments()
        .iter()
        .any(|arg| matches!(arg.ty(), NodeType::Restarg | NodeType::ForwardArg))
}

fn expandable_arguments(arguments: &[Node<'_>]) -> bool {
    arguments.iter().any(|arg| {
        matches!(
            arg.ty(),
            NodeType::Splat | NodeType::ForwardedArgs | NodeType::ForwardedRestarg
        )
    })
}

fn positional_arguments(arguments: &[Node<'_>]) -> usize {
    arguments
        .iter()
        .filter(|arg| {
            !matches!(
                arg.ty(),
                NodeType::Hash | NodeType::Kwsplat | NodeType::BlockPass | NodeType::ForwardedKwrestarg
            )
        })
        .count()
}

fn positional_parameters(def_node: Node<'_>) -> usize {
    def_node
        .arguments()
        .iter()
        .filter(|arg| matches!(arg.ty(), NodeType::Arg | NodeType::Optarg))
        .count()
}

fn extra_keyword_arguments(arguments: &[Node<'_>], def_node: Node<'_>) -> bool {
    if variadic_keyword_parameters(def_node) {
        return false;
    }
    if expandable_keyword_arguments(arguments) {
        return false;
    }

    let keyword_parameters = keyword_parameters(def_node);
    passed_keyword_arguments(arguments)
        .iter()
        .any(|k| !keyword_parameters.contains(k))
}

fn variadic_keyword_parameters(def_node: Node<'_>) -> bool {
    def_node
        .arguments()
        .iter()
        .any(|arg| matches!(arg.ty(), NodeType::Kwrestarg | NodeType::ForwardArg))
}

fn expandable_keyword_arguments(arguments: &[Node<'_>]) -> bool {
    arguments.iter().any(|arg| {
        arg.is(NodeType::ForwardedArgs)
            || arg
                .child_nodes()
                .any(|c| matches!(c.ty(), NodeType::Kwsplat | NodeType::ForwardedKwrestarg))
    })
}

fn passed_keyword_arguments<'a>(arguments: &[Node<'a>]) -> Vec<&'a str> {
    arguments
        .iter()
        .filter(|arg| arg.is(NodeType::Hash))
        .flat_map(|arg| {
            arg.pairs().into_iter().filter_map(|pair| {
                pair.key()
                    .filter(|k| k.is(NodeType::Sym))
                    .and_then(|k| k.sym_value())
            })
        })
        .collect()
}

fn keyword_parameters<'a>(def_node: Node<'a>) -> Vec<&'a str> {
    def_node
        .arguments()
        .into_iter()
        .filter(|arg| matches!(arg.ty(), NodeType::Kwarg | NodeType::Kwoptarg))
        .filter_map(|arg| arg.child_sym(0))
        .collect()
}

fn argument_match(send_arg: Node<'_>, def_arg: Node<'_>) -> bool {
    let def_arg_name = def_arg.child_sym(0);
    match def_arg.ty() {
        NodeType::Arg | NodeType::Restarg => send_arg.source() == def_arg.source(),
        NodeType::Optarg => Some(send_arg.source()) == def_arg_name,
        NodeType::Kwoptarg | NodeType::Kwarg => {
            keyword_hash_argument(send_arg)
                && send_arg
                    .pairs()
                    .into_iter()
                    .any(|pair| passing_keyword_arg(pair, def_arg_name.unwrap_or_default()))
        }
        NodeType::Kwrestarg => {
            if !keyword_hash_argument(send_arg) {
                return false;
            }
            send_arg
                .child_nodes()
                .filter(|c| matches!(c.ty(), NodeType::Kwsplat | NodeType::ForwardedKwrestarg))
                .any(|child| child.source() == def_arg.source())
        }
        NodeType::ForwardArg => send_arg.is(NodeType::ForwardedArgs),
        _ => false,
    }
}

fn keyword_hash_argument(send_arg: Node<'_>) -> bool {
    send_arg.is(NodeType::Hash) && !send_arg.has_braces()
}
