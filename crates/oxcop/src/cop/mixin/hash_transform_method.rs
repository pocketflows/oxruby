//! Port of `lib/rubocop/cop/mixin/hash_transform_method.rb` and
//! `hash_transform_method/autocorrection.rb`: shared by
//! `Style/HashTransformKeys` and `Style/HashTransformValues`.

use crate::ast::node_pattern::{Captures, MatchContext, NodePattern, Val};
use crate::ast::{Node, NodeType};
use crate::cop::{Corrector, Investigation};

pub const RESTRICT_ON_SEND: &[&str] = &["[]", "to_h"];

static HASH_RECEIVER: NodePattern = NodePattern::new(
    "{(hash ...)
     (send _ {:to_h :to_hash :merge :merge! :update :invert :except :tally} ...)
     (block (send _ {:group_by :to_h :tally :transform_keys :transform_keys!
                     :transform_values :transform_values!}) ...)
     (block (send _ :each_with_object (hash)) ...)}",
);

/// What the including cop defines.
pub struct HashTransform {
    pub new_method_name: &'static str,
    pub on_bad_each_with_object: &'static NodePattern,
    pub on_bad_hash_brackets_map: &'static NodePattern,
    pub on_bad_map_to_h: &'static NodePattern,
    pub on_bad_to_h: &'static NodePattern,
    /// `extract_captures(match)`: the indexes of the transformed argument
    /// name, the transforming and the unchanged body expressions.
    pub captures: [usize; 3],
}

/// `Captures`: match data.
struct MatchCaptures<'a> {
    transformed_argname: &'a str,
    transforming_body_expr: Node<'a>,
    unchanged_body_expr: Node<'a>,
}

impl MatchCaptures<'_> {
    fn noop_transformation(&self) -> bool {
        self.transforming_body_expr.is(NodeType::Lvar)
            && self.transforming_body_expr.name() == Some(self.transformed_argname)
            && self.transforming_body_expr.children().len() == 1
    }

    fn transformation_uses_both_args(&self) -> bool {
        self.transforming_body_expr
            .descendants()
            .any(|d| d.same_as(self.unchanged_body_expr))
    }

    fn use_transformed_argname(&self) -> bool {
        self.transforming_body_expr
            .descendants()
            .any(|n| n.is(NodeType::Lvar) && n.source() == self.transformed_argname)
    }
}

fn hash_receiver(node: Node<'_>) -> bool {
    HASH_RECEIVER.is_match(node)
}

fn matcher<'a>(pattern: &NodePattern, node: Node<'a>) -> Option<Captures<'a>> {
    let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| {
        name == "hash_receiver?" && val.node().is_some_and(hash_receiver)
    };
    let ctx = MatchContext {
        call: Some(&call),
        ..Default::default()
    };
    pattern.match_with(node, &ctx)
}

impl HashTransform {
    pub fn on_block(&self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        if let Some(m) = matcher(self.on_bad_each_with_object, node) {
            self.handle_possible_offense(cx, node, &m, "each_with_object");
        }
        if cx.target_ruby_version() < 2.6 {
            return;
        }
        if let Some(m) = matcher(self.on_bad_to_h, node) {
            self.handle_possible_offense(cx, node, &m, "to_h {...}");
        }
    }

    pub fn on_send(&self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        if let Some(m) = matcher(self.on_bad_hash_brackets_map, node) {
            self.handle_possible_offense(cx, node, &m, "Hash[_.map {...}]");
        }
        self.on_csend(cx, node);
    }

    pub fn on_csend(&self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        if let Some(m) = matcher(self.on_bad_map_to_h, node) {
            self.handle_possible_offense(cx, node, &m, "map {...}.to_h");
        }
    }

    fn extract_captures<'a>(&self, m: &Captures<'a>) -> Option<MatchCaptures<'a>> {
        let [argname, transforming, unchanged] = self.captures;
        Some(MatchCaptures {
            transformed_argname: m[argname].sym()?,
            transforming_body_expr: m[transforming].node()?,
            unchanged_body_expr: m[unchanged].node()?,
        })
    }

    fn handle_possible_offense(
        &self,
        cx: &mut Investigation<'_, '_>,
        node: Node<'_>,
        m: &Captures<'_>,
        match_desc: &str,
    ) {
        let Some(captures) = self.extract_captures(m) else {
            return;
        };
        // If key didn't actually change either, this is most likely a false
        // positive (receiver isn't a hash).
        if captures.noop_transformation() {
            return;
        }
        // Can't `transform_keys` if key transformation uses value, or
        // `transform_values` if value transformation uses key.
        if captures.transformation_uses_both_args() {
            return;
        }
        if !captures.use_transformed_argname() {
            return;
        }
        // A splat transforming expression (e.g. `[k, *v]`) can't be used as a
        // standalone block return value.
        if captures.transforming_body_expr.is(NodeType::Splat) {
            return;
        }
        let message = format!("Prefer `{}` over `{match_desc}`.", self.new_method_name);
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if let Some(correction) = self.prepare_correction(node) {
                self.execute_correction(corrector, node, &correction);
            }
        });
    }

    fn prepare_correction<'a>(&self, node: Node<'a>) -> Option<Autocorrection<'a>> {
        if let Some(m) = matcher(self.on_bad_each_with_object, node) {
            Some(Autocorrection::from_each_with_object(node, m))
        } else if let Some(m) = matcher(self.on_bad_hash_brackets_map, node) {
            Some(Autocorrection::from_hash_brackets_map(node, m))
        } else if let Some(m) = matcher(self.on_bad_map_to_h, node) {
            Some(Autocorrection::from_map_to_h(node, m))
        } else {
            matcher(self.on_bad_to_h, node).map(|m| Autocorrection::from_to_h(node, m))
        }
    }

    fn execute_correction(
        &self,
        corrector: &mut Corrector,
        node: Node<'_>,
        correction: &Autocorrection<'_>,
    ) {
        correction.strip_prefix_and_suffix(node, corrector);
        correction.set_new_method_name(self.new_method_name, corrector);
        let Some(captures) = self.extract_captures(&correction.matched) else {
            return;
        };
        correction.set_new_arg_name(captures.transformed_argname, corrector);
        correction.set_new_body_expression(captures.transforming_body_expr, corrector);
    }
}

/// `Autocorrection`: holds autocorrect data.
struct Autocorrection<'a> {
    matched: Captures<'a>,
    block_node: Node<'a>,
    leading: usize,
    trailing: usize,
}

impl<'a> Autocorrection<'a> {
    fn from_each_with_object(node: Node<'a>, matched: Captures<'a>) -> Self {
        Self {
            matched,
            block_node: node,
            leading: 0,
            trailing: 0,
        }
    }

    fn from_hash_brackets_map(node: Node<'a>, matched: Captures<'a>) -> Self {
        Self {
            matched,
            block_node: node.child_nodes().last().unwrap(),
            leading: "Hash[".len(),
            trailing: "]".len(),
        }
    }

    fn from_map_to_h(node: Node<'a>, matched: Captures<'a>) -> Self {
        let first = node.child(0).unwrap();
        let strip_trailing_chars = if node
            .parent()
            .is_some_and(|p| p.is(NodeType::Block) && p.send_node() == Some(node))
        {
            0
        } else {
            node.source_range().end_pos - first.source_range().end_pos
        };
        Self {
            matched,
            block_node: first,
            leading: 0,
            trailing: strip_trailing_chars,
        }
    }

    fn from_to_h(node: Node<'a>, matched: Captures<'a>) -> Self {
        Self {
            matched,
            block_node: node,
            leading: 0,
            trailing: 0,
        }
    }

    fn strip_prefix_and_suffix(&self, node: Node<'_>, corrector: &mut Corrector) {
        let expression = node.source_range();
        corrector.remove_leading(expression, self.leading);
        corrector.remove_trailing(expression, self.trailing);
    }

    fn set_new_method_name(&self, new_method_name: &str, corrector: &mut Corrector) {
        let send_node = self.block_node.send_node().unwrap();
        let mut range = send_node.loc().selector.unwrap();
        if let Some(send_end) = send_node.loc().end {
            // If there are arguments (only true in the `each_with_object`
            // case).
            range = range.begin().join(send_end);
        }
        corrector.replace(range, new_method_name);
    }

    fn set_new_arg_name(&self, transformed_argname: &str, corrector: &mut Corrector) {
        corrector.replace(
            self.block_node.child(1).unwrap().source_range(),
            format!("|{transformed_argname}|"),
        );
    }

    fn set_new_body_expression(&self, transforming_body_expr: Node<'_>, corrector: &mut Corrector) {
        let mut body_source = transforming_body_expr.source().to_owned();
        if transforming_body_expr.is(NodeType::Hash) && !transforming_body_expr.has_braces() {
            body_source = format!("{{ {body_source} }}");
        }
        corrector.replace(self.block_node.body().unwrap().source_range(), body_source);
    }
}
