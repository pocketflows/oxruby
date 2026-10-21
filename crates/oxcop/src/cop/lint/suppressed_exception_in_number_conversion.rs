//! Port of `lib/rubocop/cop/lint/suppressed_exception_in_number_conversion.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const EXPECTED_EXCEPTION_CLASSES: &[&str] =
    &["ArgumentError", "TypeError", "::ArgumentError", "::TypeError"];

static NUMERIC_CONSTRUCTOR_RESCUE_NIL: NodePattern = NodePattern::new(
    "(rescue
       $#numeric_method?
       (resbody nil? nil? (nil)) nil?)",
);
static BEGIN_NUMERIC_CONSTRUCTOR_RESCUE_NIL: NodePattern = NodePattern::new(
    "(kwbegin
       (rescue
         $#numeric_method?
         (resbody $_? nil?
           {(nil) nil?}) nil?))",
);
static NUMERIC_METHOD: NodePattern = NodePattern::new(
    "{
       (call #constructor_receiver? {:Integer :BigDecimal :Complex :Rational}
         _ _?)
       (call #constructor_receiver? :Float
         _)
     }",
);
static CONSTRUCTOR_RECEIVER: NodePattern =
    NodePattern::new("{nil? (const {nil? cbase} :Kernel)}");

fn call(name: &str, val: &Val<'_>, _args: &[Val<'_>]) -> bool {
    match name {
        "numeric_method?" => val.node().is_some_and(|node| {
            let ctx = MatchContext {
                call: Some(&call),
                ..Default::default()
            };
            NUMERIC_METHOD.match_with(node, &ctx).is_some()
        }),
        "constructor_receiver?" => CONSTRUCTOR_RECEIVER
            .match_val(*val, &MatchContext::default())
            .is_some(),
        _ => false,
    }
}

#[derive(Default)]
pub struct SuppressedExceptionInNumberConversion;

impl Cop for SuppressedExceptionInNumberConversion {
    fn name(&self) -> &'static str {
        "Lint/SuppressedExceptionInNumberConversion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn minimum_target_ruby_version(&self) -> Option<f64> {
        Some(2.6)
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Rescue]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        let begin_match = node
            .parent()
            .and_then(|p| BEGIN_NUMERIC_CONSTRUCTOR_RESCUE_NIL.match_with(p, &ctx));
        let (node, method) = if let Some(captures) = begin_match {
            if !expected_exception_classes_only(captures[1].nodes().first().copied()) {
                return;
            }
            let Some(method) = captures[0].node() else {
                return;
            };
            (node.parent().unwrap_or(node), method)
        } else {
            let Some(method) = NUMERIC_CONSTRUCTOR_RESCUE_NIL
                .match_with(node, &ctx)
                .and_then(|c| c[0].node())
            else {
                return;
            };
            (node, method)
        };
        if exception_keyword_argument(method) {
            return;
        }

        let mut arguments: Vec<String> = method
            .arguments()
            .iter()
            .map(|a| a.source().to_owned())
            .collect();
        arguments.push("exception: false".to_owned());
        let mut prefer = format!(
            "{}({})",
            method.method_name().unwrap_or_default(),
            arguments.join(", ")
        );
        if let Some(receiver) = method.receiver() {
            let dot = method.loc().dot.map_or("", |d| method.range_source(d));
            prefer = format!("{}{dot}{prefer}", receiver.source());
        }

        let message = format!("Use `{prefer}` instead.");
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            corrector.replace(node.source_range(), &prefer);
        });
    }
}

fn exception_keyword_argument(method: Node<'_>) -> bool {
    method.arguments().iter().any(|argument| {
        argument.is(NodeType::Hash)
            && argument.pairs().iter().any(|pair| {
                pair.key()
                    .is_some_and(|k| k.is(NodeType::Sym) && k.sym_value() == Some("exception"))
            })
    })
}

fn expected_exception_classes_only(arguments: Option<Node<'_>>) -> bool {
    let Some(arguments) = arguments else {
        return true;
    };

    // `arguments.values`: the elements of the exception list.
    let values: Vec<Node<'_>> = if arguments.is(NodeType::Array) {
        arguments.child_nodes().collect()
    } else {
        vec![arguments]
    };
    values
        .iter()
        .all(|v| EXPECTED_EXCEPTION_CLASSES.contains(&v.source()))
}
