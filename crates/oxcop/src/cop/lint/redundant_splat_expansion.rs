//! Port of `lib/rubocop/cop/lint/redundant_splat_expansion.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::Range;

const MSG: &str = "Replace splat expansion with comma separated values.";
const ARRAY_PARAM_MSG: &str = "Pass array contents as separate arguments.";
const PERCENT_W: &str = "%w";
const PERCENT_CAPITAL_W: &str = "%W";
const PERCENT_I: &str = "%i";
const PERCENT_CAPITAL_I: &str = "%I";
const ASSIGNMENT_TYPES: &[NodeType] = &[
    NodeType::Lvasgn,
    NodeType::Ivasgn,
    NodeType::Cvasgn,
    NodeType::Gvasgn,
    NodeType::Casgn,
];

static ARRAY_NEW: NodePattern = NodePattern::new(
    "{
       $(send (const {nil? cbase} :Array) :new ...)
       $(block (send (const {nil? cbase} :Array) :new ...) ...)
     }",
);

static LITERAL_EXPANSION: NodePattern = NodePattern::new(
    "(splat {$({str dstr int float array} ...) (block $#array_new? ...) $#array_new?} ...)",
);

#[derive(Default)]
pub struct RedundantSplatExpansion;

impl Cop for RedundantSplatExpansion {
    fn name(&self) -> &'static str {
        "Lint/RedundantSplatExpansion"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Splat]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if !redundant_splat_expansion(node) {
            return;
        }

        if array_splat(node) && (method_argument(node) || part_of_an_array(node)) {
            if allow_percent_literal_array_argument(cx) && use_percent_literal_array_argument(node)
            {
                return;
            }
            cx.add_offense_with_correction(node.source_range(), ARRAY_PARAM_MSG, |corrector| {
                autocorrect(corrector, node);
            });
        } else {
            cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                autocorrect(corrector, node);
            });
        }
    }
}

fn array_new(node: Node<'_>) -> bool {
    ARRAY_NEW.is_match(node)
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>) {
    if let Some((range, content)) = replacement_range_and_content(node) {
        corrector.replace(range, content);
    }
}

fn redundant_splat_expansion(node: Node<'_>) -> bool {
    let call = |name: &str, val: &Val<'_>, _args: &[Val<'_>]| {
        name == "array_new?" && val.node().is_some_and(array_new)
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    let Some(captures) = LITERAL_EXPANSION.match_with(node, &ctx) else {
        return false;
    };
    let Some(expanded_item) = captures.iter().find_map(|c| c.node()) else {
        return false;
    };

    if expanded_item.is(NodeType::Send) {
        if array_new_inside_array_literal(expanded_item) {
            return false;
        }
        let grandparent = node.parent().and_then(|p| p.parent());
        if let Some(grandparent) = grandparent
            && !ASSIGNMENT_TYPES.contains(&grandparent.ty())
        {
            return false;
        }
    } else if expanded_item.is(NodeType::Array) && expanded_item.children().is_empty() {
        return false;
    }
    true
}

fn array_new_inside_array_literal(array_new_node: Node<'_>) -> bool {
    if !array_new(array_new_node) {
        return false;
    }
    let Some(grandparent) = array_new_node.parent().and_then(|p| p.parent()) else {
        return false;
    };
    grandparent.is(NodeType::Array) && grandparent.children().len() > 1
}

fn replacement_range_and_content(node: Node<'_>) -> Option<(Range, String)> {
    let variable = node.child(0)?;
    let mut expression = node.source_range();
    let parent = node.parent()?;

    if array_new(variable) {
        if parent.is(NodeType::Array) {
            expression = parent.source_range();
        }
        Some((expression, variable.source().to_owned()))
    } else if !variable.is(NodeType::Array) {
        let mut replacement = variable.source().to_owned();
        if wrap_in_brackets(node) {
            replacement = format!("[{replacement}]");
        }
        Some((expression, replacement))
    } else if redundant_brackets(node) {
        Some((expression, remove_brackets(variable)))
    } else {
        Some((node.loc().operator?, String::new()))
    }
}

fn array_splat(node: Node<'_>) -> bool {
    node.child(0).is_some_and(|c| c.is(NodeType::Array))
}

fn method_argument(node: Node<'_>) -> bool {
    node.parent().is_some_and(|p| p.is_call())
}

fn part_of_an_array(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        parent.is(NodeType::Array) && parent.loc().begin.is_some() && parent.loc().end.is_some()
    })
}

fn redundant_brackets(node: Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    let grandparent = parent.parent();
    parent.is(NodeType::When)
        || method_argument(node)
        || part_of_an_array(node)
        || grandparent.is_some_and(|g| g.is(NodeType::Resbody))
}

fn wrap_in_brackets(node: Node<'_>) -> bool {
    node.parent()
        .is_some_and(|p| p.is(NodeType::Array) && !p.is_bracketed())
}

fn remove_brackets(array: Node<'_>) -> String {
    let array_start = array
        .loc()
        .begin
        .map_or("", |b| array.range_source(b));
    let elements: Vec<&str> = array.child_nodes().map(|e| e.source()).collect();

    if array_start.starts_with(PERCENT_W) {
        format!("'{}'", elements.join("', '"))
    } else if array_start.starts_with(PERCENT_CAPITAL_W) {
        format!("\"{}\"", elements.join("\", \""))
    } else if array_start.starts_with(PERCENT_I) {
        format!(":{}", elements.join(", :"))
    } else if array_start.starts_with(PERCENT_CAPITAL_I) {
        format!(":\"{}\"", elements.join("\", :\""))
    } else {
        elements.join(", ")
    }
}

fn use_percent_literal_array_argument(node: Node<'_>) -> bool {
    let Some(argument) = node.child(0) else {
        return false;
    };
    method_argument(node)
        && (argument.is_percent_literal_of("string") || argument.is_percent_literal_of("symbol"))
}

fn allow_percent_literal_array_argument(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_bool("AllowPercentLiteralArrayArgument", true)
}
