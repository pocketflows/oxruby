//! Port of `lib/rubocop/cop/layout/space_around_operators.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::preceding_following_alignment::{
    Aligned, PrecedingFollowingAlignment, allow_for_alignment,
};
use crate::cop::mixin::range_help::{SurroundingSpace, range_with_surrounding_space};
use crate::cop::mixin::rational_literal::rational_literal;
use crate::cop::{Cop, Corrector, Investigation};
use crate::processed_source::Range;
use crate::regex;

const IRREGULAR_METHODS: &[&str] = &["[]", "!", "[]="];
const EXCESSIVE_SPACE: &str = "  ";

/// The `type` argument of `check_operator`, as far as the cop looks at it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OperatorType {
    Assignment,
    SpecialAsgn,
    Other,
}

#[derive(Default)]
pub struct SpaceAroundOperators {
    alignment: PrecedingFollowingAlignment,
}

impl Cop for SpaceAroundOperators {
    fn name(&self) -> &'static str {
        "Layout/SpaceAroundOperators"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &["Style/SelfAssignment"]
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[
            Sclass, Pair, If, Resbody, Send, Lvasgn, Casgn, Masgn, Ivasgn, Cvasgn, Gvasgn, OrAsgn,
            AndAsgn, OpAsgn, Class, Def, Defs, Or, And, MatchPattern, MatchAlt, MatchAs,
        ]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        use NodeType as T;
        use OperatorType::*;
        let loc = node.loc();
        match node.ty() {
            T::Sclass => self.check_operator(cx, Other, loc.operator, Some(node)),
            T::Pair => {
                if !node.is_hash_rocket() {
                    return;
                }
                if hash_table_style(cx) && !node.parent().is_some_and(pairs_on_same_line) {
                    return;
                }
                self.check_operator(cx, Other, loc.operator, Some(node));
            }
            T::If => {
                if !node.is_ternary() {
                    return;
                }
                self.check_operator(cx, Other, loc.question, node.if_branch());
                self.check_operator(cx, Other, loc.colon, node.else_branch());
            }
            T::Resbody => {
                if loc.assoc.is_none() {
                    return;
                }
                self.check_operator(cx, Other, loc.assoc, node.exception_variable());
            }
            T::Send => {
                if rational_literal(node) {
                    return;
                }
                if node.is_setter_method() {
                    if let Some(rhs) = node.first_argument() {
                        self.check_operator(cx, SpecialAsgn, loc.operator, Some(rhs));
                    }
                } else if regular_operator(node) {
                    self.check_operator(cx, Other, loc.selector, node.first_argument());
                }
            }
            T::Class => {
                if let Some(rhs) = node.parent_class() {
                    self.check_operator(cx, Other, loc.operator, Some(rhs));
                }
            }
            T::Def | T::Defs => {
                if node.is_endless() {
                    self.check_operator(cx, Assignment, loc.assignment, node.body());
                }
            }
            T::Or | T::And => {
                if let Some(rhs) = node.rhs() {
                    self.check_operator(cx, Other, loc.operator, Some(rhs));
                }
            }
            T::MatchPattern => {
                if cx.target_ruby_version() < 3.0 {
                    return;
                }
                self.check_operator(cx, Other, loc.operator, Some(node));
            }
            T::MatchAlt | T::MatchAs => self.check_operator(cx, Other, loc.operator, Some(node)),
            _ => {
                // `on_assignment`.
                let Some(rhs) = node.rhs() else {
                    return;
                };
                let ty = if node.is(T::OpAsgn) {
                    SpecialAsgn
                } else {
                    Assignment
                };
                self.check_operator(cx, ty, loc.operator, Some(rhs));
            }
        }
    }
}

fn regular_operator(send_node: Node<'_>) -> bool {
    if send_node.is_unary_operation() || send_node.is_dot() || send_node.is_double_colon() {
        return false;
    }
    send_node.is_operator_method()
        && !send_node
            .method_name()
            .is_some_and(|m| IRREGULAR_METHODS.contains(&m))
}

fn pairs_on_same_line(hash: Node<'_>) -> bool {
    let pairs = hash.pairs();
    pairs
        .windows(2)
        .any(|w| w[0].last_line() == w[1].first_line())
}

impl SpaceAroundOperators {
    fn check_operator(
        &mut self,
        cx: &mut Investigation<'_, '_>,
        ty: OperatorType,
        operator: Option<Range>,
        right_operand: Option<Node<'_>>,
    ) {
        let Some(operator) = operator else {
            return;
        };
        let ps = cx.processed_source;
        let with_space =
            range_with_surrounding_space(ps.buffer_source(), operator, SurroundingSpace::default());
        if ps.slice(with_space).starts_with('\n') {
            return;
        }
        if let Some(comment) = ps.comment_at_line(ps.line_of(operator.begin_pos))
            && ps.column_of(with_space.end_pos) == ps.column_of(comment.range.begin_pos)
        {
            return;
        }
        let Some(msg) = self.offense_message(cx, ty, operator, with_space, right_operand) else {
            return;
        };
        let rational = right_operand.is_some_and(|r| r.is(NodeType::Rational));
        cx.add_offense_cx(operator, &msg, |corrector, cx| {
            autocorrect(corrector, cx, with_space, rational);
        });
    }

    fn offense_message(
        &mut self,
        cx: &Investigation<'_, '_>,
        ty: OperatorType,
        operator: Range,
        with_space: Range,
        right_operand: Option<Node<'_>>,
    ) -> Option<String> {
        let ps = cx.processed_source;
        let operator_source = ps.slice(operator);
        let with_space_source = ps.slice(with_space);
        let rational = right_operand.is_some_and(|r| r.is(NodeType::Rational));
        if should_not_have_surrounding_space(cx, operator_source, rational) {
            if with_space_source == operator_source {
                return None;
            }
            Some(format!("Space around operator `{operator_source}` detected."))
        } else if regex!(r"^\s.*\s$").find(with_space_source).is_none() {
            Some(format!(
                "Surrounding space missing for operator `{operator_source}`."
            ))
        } else if self.excess_leading_space(cx, ty, operator, with_space)
            || right_operand.is_some_and(|r| self.excess_trailing_space(cx, r.source_range(), with_space))
        {
            Some(format!(
                "Operator `{operator_source}` should be surrounded by a single space."
            ))
        } else {
            None
        }
    }

    fn excess_leading_space(
        &mut self,
        cx: &Investigation<'_, '_>,
        ty: OperatorType,
        operator: Range,
        with_space: Range,
    ) -> bool {
        let ps = cx.processed_source;
        if !allow_for_alignment(cx) {
            return false;
        }
        if !ps.slice(with_space).starts_with(EXCESSIVE_SPACE) {
            return false;
        }
        if !grouped_alignment_type(cx, ty) {
            return !self.alignment.aligned_with_operator(ps, operator);
        }
        let align_preceding = self.alignment.aligned_with_preceding_equals_operator(ps, operator);
        if align_preceding == Aligned::Yes
            || self.alignment.aligned_with_subsequent_equals_operator(ps, operator) == Aligned::None
        {
            return false;
        }
        self.alignment.aligned_with_subsequent_equals_operator(ps, operator) != Aligned::Yes
    }

    fn excess_trailing_space(
        &mut self,
        cx: &Investigation<'_, '_>,
        right_operand: Range,
        with_space: Range,
    ) -> bool {
        let ps = cx.processed_source;
        ps.slice(with_space).ends_with(EXCESSIVE_SPACE)
            && (!allow_for_alignment(cx) || !self.alignment.aligned_with_something(ps, right_operand))
    }
}

fn autocorrect(corrector: &mut Corrector, cx: &Investigation<'_, '_>, range: Range, rational: bool) {
    let range_source = cx.source(range);
    let operator = range_source.trim();
    if operator == "**" && !space_around_exponent_operator(cx) {
        corrector.replace(range, "**");
    } else if operator == "/" && !space_around_slash_operator(cx, rational) {
        corrector.replace(range, "/");
    } else if range_source.ends_with('\n') {
        corrector.replace(range, format!(" {operator}\n"));
    } else if force_equal_sign_alignment(cx) && !range_source.ends_with(' ') {
        corrector.insert_after(range, " ");
    } else {
        corrector.replace(range, format!(" {operator} "));
    }
}

fn grouped_alignment_type(cx: &Investigation<'_, '_>, ty: OperatorType) -> bool {
    ty == OperatorType::Assignment
        || (ty == OperatorType::SpecialAsgn && force_equal_sign_alignment(cx))
}

fn hash_table_style(cx: &Investigation<'_, '_>) -> bool {
    match cx
        .config
        .for_cop("Layout/HashAlignment")
        .get("EnforcedHashRocketStyle")
    {
        Some(serde_yaml_ng::Value::String(s)) => s == "table",
        Some(serde_yaml_ng::Value::Sequence(styles)) => {
            styles.iter().any(|s| s.as_str() == Some("table"))
        }
        _ => false,
    }
}

fn space_around_exponent_operator(cx: &Investigation<'_, '_>) -> bool {
    cx.cop_config_str("EnforcedStyleForExponentOperator") == Some("space")
}

fn space_around_slash_operator(cx: &Investigation<'_, '_>, rational: bool) -> bool {
    if !rational {
        return true;
    }
    cx.cop_config_str("EnforcedStyleForRationalLiterals") == Some("space")
}

fn force_equal_sign_alignment(cx: &Investigation<'_, '_>) -> bool {
    match cx
        .config
        .for_cop("Layout/ExtraSpacing")
        .get("ForceEqualSignAlignment")
    {
        None | Some(serde_yaml_ng::Value::Null) | Some(serde_yaml_ng::Value::Bool(false)) => false,
        Some(_) => true,
    }
}

fn should_not_have_surrounding_space(cx: &Investigation<'_, '_>, operator: &str, rational: bool) -> bool {
    match operator {
        "**" => !space_around_exponent_operator(cx),
        "/" => !space_around_slash_operator(cx, rational),
        _ => false,
    }
}
