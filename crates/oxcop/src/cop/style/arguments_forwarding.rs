//! Port of `lib/rubocop/cop/style/arguments_forwarding.rb`.

use serde_yaml_ng::Value;

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::util::{add_parentheses, parentheses};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const FORWARDING_MSG: &str = "Use shorthand syntax `...` for arguments forwarding.";
const ARGS_MSG: &str = "Use anonymous positional arguments forwarding (`*`).";
const KWARGS_MSG: &str = "Use anonymous keyword arguments forwarding (`**`).";
const BLOCK_MSG: &str = "Use anonymous block arguments forwarding (`&`).";

static FORWARDED_REST_ARG: NodePattern = NodePattern::new("(splat (lvar %1))");
static EXTRACT_FORWARDED_KWREST_ARG: NodePattern =
    NodePattern::new("(hash <$(kwsplat (lvar %1)) ...>)");
static FORWARDED_BLOCK_ARG: NodePattern = NodePattern::new("(block_pass {(lvar %1) nil?})");
static DEF_ALL_ANONYMOUS_ARGS: NodePattern = NodePattern::new(
    "(
       def _
       (args ... (restarg) (kwrestarg) (blockarg nil?))
       _
     )",
);
static SEND_ALL_ANONYMOUS_ARGS: NodePattern = NodePattern::new(
    "(
       send _ _
       ... (forwarded_restarg) (hash (forwarded_kwrestarg)) (block_pass nil?)
     )",
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Classification {
    All,
    AllAnonymous,
    RestOrKwrest,
}

struct Classified<'a> {
    send_node: Node<'a>,
    classification: Classification,
    forward_rest: Option<Node<'a>>,
    forward_kwrest: Option<Node<'a>>,
    forward_block_arg: Option<Node<'a>>,
}

#[derive(Default)]
pub struct ArgumentsForwarding;

impl Cop for ArgumentsForwarding {
    fn name(&self) -> &'static str {
        "Style/ArgumentsForwarding"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Naming/BlockForwarding", "Style/MethodDefParentheses"]
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.7)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(body) = node.body() else {
            return;
        };
        let args = node.arguments();
        let find = |ty: NodeType| args.iter().copied().find(|a| a.is(ty));
        let forwardable_args = [
            redundant_named_arg(cx, find(NodeType::Restarg), "RedundantRestArgumentNames", "*"),
            redundant_named_arg(
                cx,
                find(NodeType::Kwrestarg),
                "RedundantKeywordRestArgumentNames",
                "**",
            ),
            redundant_named_arg(cx, find(NodeType::Blockarg), "RedundantBlockArgumentNames", "&"),
        ];
        let send_nodes: Vec<Node<'_>> = node
            .descendants()
            .filter(|d| d.is_call() || matches!(d.ty(), NodeType::Super | NodeType::Yield))
            .collect();
        let referenced_lvars = non_splat_or_block_pass_lvar_references(body);
        let classifier_config = ClassifierConfig {
            target_ruby_version: cx.target_ruby_version(),
            allow_only_rest_arguments: cx.cop_config_bool("AllowOnlyRestArgument", true),
        };
        let classifications: Vec<Classified<'_>> = send_nodes
            .into_iter()
            .filter_map(|send_node| {
                let classifier = SendNodeClassifier {
                    def_node: node,
                    send_node,
                    referenced_lvars: &referenced_lvars,
                    forwardable_args,
                    config: classifier_config,
                };
                Some(Classified {
                    send_node,
                    classification: classifier.classification()?,
                    forward_rest: classifier.forwarded_rest_arg(),
                    forward_kwrest: classifier.forwarded_kwrest_arg(),
                    forward_block_arg: classifier.forwarded_block_arg(),
                })
            })
            .collect();
        if classifications.is_empty() {
            return;
        }
        if classifications.iter().all(|c| {
            matches!(
                c.classification,
                Classification::All | Classification::AllAnonymous
            )
        }) {
            add_forward_all_offenses(cx, node, &classifications, forwardable_args);
        } else if cx.target_ruby_version() >= 3.2 {
            add_post_ruby_32_offenses(cx, node, &classifications, forwardable_args);
        }
    }
}

fn add_forward_all_offenses(
    cx: &mut Investigation<'_, '_>,
    node: Node<'_>,
    classifications: &[Classified<'_>],
    forwardable_args: [Option<Node<'_>>; 3],
) {
    let [rest_arg, kwrest_arg, block_arg] = forwardable_args;
    let args_node = node.arguments_node().unwrap();
    for c in classifications {
        if c.forward_rest.is_none()
            && c.forward_kwrest.is_none()
            && c.classification != Classification::AllAnonymous
        {
            if allow_anonymous_forwarding_in_block(cx, c.forward_block_arg) {
                register_forward_block_arg_offense(cx, true, args_node, block_arg);
                register_forward_block_arg_offense(cx, true, c.send_node, c.forward_block_arg);
            }
            return;
        }
        let first_arg = c
            .forward_rest
            .or(c.forward_kwrest)
            .or_else(|| forward_all_first_argument(c.send_node));
        if let Some(first_arg) = first_arg {
            register_forward_all_offense(cx, c.send_node, c.send_node, first_arg);
        }
    }
    if let Some(first) = rest_arg.or(kwrest_arg) {
        register_forward_all_offense(cx, node, args_node, first);
    }
}

fn forward_all_first_argument(node: Node<'_>) -> Option<Node<'_>> {
    node.arguments()
        .into_iter()
        .rev()
        .find(|a| a.is(NodeType::ForwardedRestarg))
}

fn add_post_ruby_32_offenses(
    cx: &mut Investigation<'_, '_>,
    def_node: Node<'_>,
    classifications: &[Classified<'_>],
    forwardable_args: [Option<Node<'_>>; 3],
) {
    if !cx.cop_config_bool("UseAnonymousForwarding", false) {
        return;
    }
    if cx.target_ruby_version() < 3.4
        && classifications
            .iter()
            .any(|c| c.send_node.ancestors().any(|a| a.is_any_block()))
    {
        return;
    }
    let [rest_arg, kwrest_arg, block_arg] = forwardable_args;
    let args_node = def_node.arguments_node().unwrap();
    for c in classifications {
        let add_parens = c.forward_rest.is_none();
        if allow_anonymous_forwarding_in_block(cx, c.forward_rest) {
            register_forward_args_offense(cx, args_node, rest_arg);
            register_forward_args_offense(cx, c.send_node, c.forward_rest);
        }
        if allow_anonymous_forwarding_in_block(cx, c.forward_kwrest) {
            register_forward_kwargs_offense(cx, add_parens, args_node, kwrest_arg);
            register_forward_kwargs_offense(cx, add_parens, c.send_node, c.forward_kwrest);
        }
        if allow_anonymous_forwarding_in_block(cx, c.forward_block_arg) {
            register_forward_block_arg_offense(cx, add_parens, args_node, block_arg);
            register_forward_block_arg_offense(cx, add_parens, c.send_node, c.forward_block_arg);
        }
    }
}

fn non_splat_or_block_pass_lvar_references<'a>(body: Node<'a>) -> Vec<&'a str> {
    let mut names: Vec<&str> = Vec::new();
    for lvar in body
        .descendants()
        .filter(|d| matches!(d.ty(), NodeType::Lvar | NodeType::Lvasgn))
    {
        if lvar.is(NodeType::Lvar)
            && lvar.parent().is_some_and(|p| {
                matches!(
                    p.ty(),
                    NodeType::Splat | NodeType::Kwsplat | NodeType::BlockPass
                )
            })
        {
            continue;
        }
        if let Some(name) = lvar.child_sym(0)
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

fn redundant_named_arg<'a>(
    cx: &Investigation<'_, '_>,
    arg: Option<Node<'a>>,
    config_name: &str,
    keyword: &str,
) -> Option<Node<'a>> {
    let arg = arg?;
    let mut names: Vec<String> = cx
        .cop_config_list(config_name)
        .iter()
        .map(|name| format!("{keyword}{name}"))
        .collect();
    names.push(keyword.to_owned());
    names.iter().any(|n| n == arg.source()).then_some(arg)
}

fn allow_anonymous_forwarding_in_block(cx: &Investigation<'_, '_>, node: Option<Node<'_>>) -> bool {
    let Some(node) = node else {
        return false;
    };
    if cx.target_ruby_version() >= 3.4 {
        return true;
    }
    !node.ancestors().any(|a| a.is_any_block())
}

fn register_forward_args_offense(
    cx: &mut Investigation<'_, '_>,
    def_arguments_or_send: Node<'_>,
    rest_arg_or_splat: Option<Node<'_>>,
) {
    let Some(rest_arg_or_splat) = rest_arg_or_splat else {
        return;
    };
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(rest_arg_or_splat.source_range(), ARGS_MSG, |corrector| {
        add_parens_if_missing(src, def_arguments_or_send, corrector);
        corrector.replace(rest_arg_or_splat.source_range(), "*");
    });
}

fn register_forward_kwargs_offense(
    cx: &mut Investigation<'_, '_>,
    add_parens: bool,
    def_arguments_or_send: Node<'_>,
    kwrest_arg_or_splat: Option<Node<'_>>,
) {
    let Some(kwrest_arg_or_splat) = kwrest_arg_or_splat else {
        return;
    };
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(kwrest_arg_or_splat.source_range(), KWARGS_MSG, |corrector| {
        if add_parens {
            add_parens_if_missing(src, def_arguments_or_send, corrector);
        }
        corrector.replace(kwrest_arg_or_splat.source_range(), "**");
    });
}

fn register_forward_block_arg_offense(
    cx: &mut Investigation<'_, '_>,
    add_parens: bool,
    def_arguments_or_send: Node<'_>,
    block_arg: Option<Node<'_>>,
) {
    let Some(block_arg) = block_arg else {
        return;
    };
    if cx.target_ruby_version() <= 3.0 || block_arg.source() == "&" || explicit_block_name(cx) {
        return;
    }
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(block_arg.source_range(), BLOCK_MSG, |corrector| {
        if add_parens {
            add_parens_if_missing(src, def_arguments_or_send, corrector);
        }
        corrector.replace(block_arg.source_range(), "&");
    });
}

fn register_forward_all_offense(
    cx: &mut Investigation<'_, '_>,
    def_or_send: Node<'_>,
    send_or_arguments: Node<'_>,
    rest_or_splat: Node<'_>,
) {
    let arg_range = arguments_range(def_or_send, rest_or_splat);
    let src = cx.processed_source.buffer_source();
    cx.add_offense_with_correction(arg_range, FORWARDING_MSG, |corrector| {
        add_parens_if_missing(src, send_or_arguments, corrector);
        corrector.replace(arg_range, "...");
    });
}

fn arguments_range(node: Node<'_>, first_node: Node<'_>) -> Range {
    first_node
        .source_range()
        .begin()
        .join(node.last_argument().unwrap().source_range().end())
}

fn add_parens_if_missing(src: &str, node: Node<'_>, corrector: &mut Corrector) {
    if parentheses(node) {
        return;
    }
    if node.is_send() && node.is_method("[]") {
        return;
    }
    add_parentheses(src, node, corrector);
}

fn explicit_block_name(cx: &Investigation<'_, '_>) -> bool {
    const COP: &str = "Naming/BlockForwarding";
    cx.config.cop_enabled(COP)
        && cx.config.for_cop(COP).get("EnforcedStyle").and_then(Value::as_str) == Some("explicit")
}

#[derive(Clone, Copy)]
struct ClassifierConfig {
    target_ruby_version: f64,
    allow_only_rest_arguments: bool,
}

struct SendNodeClassifier<'a, 'r> {
    def_node: Node<'a>,
    send_node: Node<'a>,
    referenced_lvars: &'r [&'a str],
    forwardable_args: [Option<Node<'a>>; 3],
    config: ClassifierConfig,
}

impl<'a> SendNodeClassifier<'a, '_> {
    fn arg_name(&self, index: usize) -> Option<&'a str> {
        self.forwardable_args[index].and_then(|a| a.name())
    }

    fn name_param(&self, index: usize) -> Param<'a> {
        self.arg_name(index).map_or(Param::Nil, Param::Sym)
    }

    fn referenced(&self, index: usize) -> bool {
        self.arg_name(index)
            .is_some_and(|name| self.referenced_lvars.contains(&name))
    }

    fn arguments(&self) -> Vec<Node<'a>> {
        self.send_node.arguments()
    }

    fn forwarded_rest_arg(&self) -> Option<Node<'a>> {
        if self.referenced(0) {
            return None;
        }
        let params = [self.name_param(0)];
        let ctx = MatchContext {
            params: &params,
            ..Default::default()
        };
        self.arguments()
            .into_iter()
            .find(|arg| FORWARDED_REST_ARG.match_with(*arg, &ctx).is_some())
    }

    fn forwarded_kwrest_arg(&self) -> Option<Node<'a>> {
        if self.referenced(1) {
            return None;
        }
        let params = [self.name_param(1)];
        let ctx = MatchContext {
            params: &params,
            ..Default::default()
        };
        self.arguments().into_iter().find_map(|arg| {
            EXTRACT_FORWARDED_KWREST_ARG
                .match_with(arg, &ctx)
                .and_then(|c| c[0].node())
        })
    }

    fn forwarded_block_arg(&self) -> Option<Node<'a>> {
        if self.referenced(2) {
            return None;
        }
        let params = [self.name_param(2)];
        let ctx = MatchContext {
            params: &params,
            ..Default::default()
        };
        self.arguments()
            .into_iter()
            .find(|arg| FORWARDED_BLOCK_ARG.match_with(*arg, &ctx).is_some())
    }

    fn classification(&self) -> Option<Classification> {
        if self.forwarded_rest_arg().is_none()
            && self.forwarded_kwrest_arg().is_none()
            && self.forwarded_block_arg().is_none()
        {
            return None;
        }
        Some(if self.ruby_32_only_anonymous_forwarding() {
            Classification::AllAnonymous
        } else if self.can_forward_all() {
            Classification::All
        } else {
            Classification::RestOrKwrest
        })
    }

    fn can_forward_all(&self) -> bool {
        if self.referenced(0) || self.referenced(1) || self.referenced(2) {
            return false;
        }
        let target = self.config.target_ruby_version;
        if target <= 3.0
            && self
                .def_node
                .arguments()
                .iter()
                .any(|a| a.is(NodeType::Optarg))
        {
            return false;
        }
        if target >= 3.2
            && !(self.forwarded_rest_arg().is_some() && self.forwarded_kwrest_arg().is_some())
        {
            return false;
        }
        let offensive_block_forwarding = if self.forwardable_args[2].is_some() {
            self.forwarded_block_arg().is_some()
        } else {
            !self.config.allow_only_rest_arguments
        };
        if !offensive_block_forwarding {
            return false;
        }
        if self.additional_kwargs_or_forwarded_kwargs() {
            return false;
        }
        self.no_additional_args() || (target >= 3.0 && self.no_post_splat_args())
    }

    fn ruby_32_only_anonymous_forwarding(&self) -> bool {
        if self.send_node.ancestors().any(|a| a.is_any_block()) {
            return false;
        }
        DEF_ALL_ANONYMOUS_ARGS.is_match(self.def_node)
            && SEND_ALL_ANONYMOUS_ARGS.is_match(self.send_node)
    }

    fn no_post_splat_args(&self) -> bool {
        let arguments = self.arguments();
        let Some(rest) = self.forwarded_rest_arg() else {
            return true;
        };
        let Some(splat_index) = arguments.iter().position(|a| a.same_as(rest)) else {
            return true;
        };
        match arguments.get(splat_index + 1) {
            None => true,
            Some(arg) => matches!(arg.ty(), NodeType::Hash | NodeType::BlockPass),
        }
    }

    fn additional_kwargs_or_forwarded_kwargs(&self) -> bool {
        let additional_kwargs = self
            .def_node
            .arguments()
            .iter()
            .any(|a| matches!(a.ty(), NodeType::Kwarg | NodeType::Kwoptarg));
        let forward_additional_kwargs = self
            .forwarded_kwrest_arg()
            .is_some_and(|k| k.parent().is_some_and(|p| p.children().len() != 1));
        additional_kwargs || forward_additional_kwargs
    }

    fn no_additional_args(&self) -> bool {
        let forwardable_count = self.forwardable_args.iter().flatten().count();
        let missing = (self.arg_name(0).is_some() && self.forwarded_rest_arg().is_none())
            || (self.arg_name(1).is_some() && self.forwarded_kwrest_arg().is_none());
        if missing {
            return false;
        }
        self.def_node.arguments().len() == forwardable_count
            && self.send_node.arguments().len() == forwardable_count
    }
}
