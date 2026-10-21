//! Port of `lib/rubocop/cop/style/trivial_accessors.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::allowed_methods::allowed_methods;
use crate::cop::{Cop, Corrector, Investigation};

static LOOKS_LIKE_TRIVIAL_WRITER: NodePattern = NodePattern::new(
    "{(def    _ (args (arg ...)) (ivasgn _ (lvar _)))
      (defs _ _ (args (arg ...)) (ivasgn _ (lvar _)))}",
);

#[derive(Default)]
pub struct TrivialAccessors;

/// The cop's configuration, read once per callback.
struct Options<'a> {
    exact_name_match: bool,
    allow_predicates: bool,
    allow_dsl_writers: bool,
    allowed_methods: Vec<&'a str>,
}

impl Cop for TrivialAccessors {
    fn name(&self) -> &'static str {
        "Style/TrivialAccessors"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Def, NodeType::Defs]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if node.parent().is_none() || in_module_or_instance_eval(node) {
            return;
        }
        if cx.cop_config_bool("IgnoreClassMethods", false) && node.is(NodeType::Defs) {
            return;
        }
        let opts = Options {
            exact_name_match: cx.cop_config_bool("ExactNameMatch", false),
            allow_predicates: cx.cop_config_bool("AllowPredicates", false),
            allow_dsl_writers: cx.cop_config_bool("AllowDSLWriters", false),
            allowed_methods: allowed_methods(cx),
        };
        let kind = if trivial_reader(&opts, node) {
            "reader"
        } else if trivial_writer(&opts, node) {
            "writer"
        } else {
            return;
        };
        let message = format!("Use `attr_{kind}` to define trivial {kind} methods.");
        cx.add_offense_with_correction(node.loc().keyword.unwrap(), &message, |corrector| {
            autocorrect(&opts, corrector, node);
        });
    }
}

fn in_module_or_instance_eval(node: Node<'_>) -> bool {
    for pnode in node.ancestors() {
        match pnode.ty() {
            NodeType::Class | NodeType::Sclass => return false,
            NodeType::Module => return true,
            _ if pnode.is_any_block() && pnode.is_method("instance_eval") => return true,
            _ => {}
        }
    }
    false
}

fn autocorrect(opts: &Options<'_>, corrector: &mut Corrector, node: Node<'_>) {
    if node.parent().is_some_and(|p| p.is_send()) {
        return;
    }
    if node.is(NodeType::Def) {
        autocorrect_instance(opts, corrector, node);
    } else if node.is(NodeType::Defs) && node.child(0).is_some_and(|c| c.is(NodeType::SelfNode)) {
        autocorrect_class(opts, corrector, node);
    }
}

fn dsl_writer(node: Node<'_>) -> bool {
    !node.is_assignment_method()
}

fn trivial_reader(opts: &Options<'_>, node: Node<'_>) -> bool {
    looks_like_trivial_reader(node)
        && !allowed_method_name(opts, node)
        && !(opts.allow_predicates && node.is_predicate_method())
}

fn looks_like_trivial_reader(node: Node<'_>) -> bool {
    !node.has_arguments() && node.body().is_some_and(|b| b.is(NodeType::Ivar))
}

fn trivial_writer(opts: &Options<'_>, node: Node<'_>) -> bool {
    LOOKS_LIKE_TRIVIAL_WRITER.is_match(node)
        && !allowed_method_name(opts, node)
        && !(opts.allow_dsl_writers && dsl_writer(node))
}

fn allowed_method_name(opts: &Options<'_>, node: Node<'_>) -> bool {
    let method_name = node.method_name().unwrap_or_default();
    method_name == "initialize"
        || opts.allowed_methods.contains(&method_name)
        || (opts.exact_name_match && !names_match(node))
}

fn names_match(node: Node<'_>) -> bool {
    let ivar_name = node
        .body()
        .and_then(|b| b.child_sym(0))
        .unwrap_or_default();
    let method_name = node.method_name().unwrap_or_default();
    let stripped = method_name
        .strip_suffix(['=', '?'])
        .unwrap_or(method_name);
    Some(stripped) == ivar_name.get(1..)
}

fn trivial_accessor_kind(opts: &Options<'_>, node: Node<'_>) -> Option<&'static str> {
    if trivial_writer(opts, node) && !dsl_writer(node) {
        Some("writer")
    } else if trivial_reader(opts, node) {
        Some("reader")
    } else {
        None
    }
}

fn accessor(kind: &str, method_name: &str) -> String {
    format!(
        "attr_{kind} :{}",
        method_name.strip_suffix('=').unwrap_or(method_name)
    )
}

fn autocorrect_instance(opts: &Options<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let Some(kind) = trivial_accessor_kind(opts, node) else {
        return;
    };
    if !names_match(node) || node.is_predicate_method() {
        return;
    }
    corrector.replace(
        node.source_range(),
        accessor(kind, node.method_name().unwrap()),
    );
}

fn autocorrect_class(opts: &Options<'_>, corrector: &mut Corrector, node: Node<'_>) {
    let Some(kind) = trivial_accessor_kind(opts, node) else {
        return;
    };
    if !names_match(node) || node.is_predicate_method() {
        return;
    }
    let indent = " ".repeat(node.column());
    corrector.replace(
        node.source_range(),
        [
            "class << self".to_owned(),
            format!("{indent}  {}", accessor(kind, node.method_name().unwrap())),
            format!("{indent}end"),
        ]
        .join("\n"),
    );
}
