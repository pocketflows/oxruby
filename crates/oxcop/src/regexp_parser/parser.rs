//! Port of `regexp_parser`'s `Regexp::Parser` (`lib/regexp_parser/parser.rb`).
//! Expression options (`active_opts`) aren't tracked: nothing uses them.

use std::collections::HashMap;

use crate::regexp_parser::expression::{
    BackrefKind, Class, ExprId, Expression, GroupKind, Quantifier, SequenceKind, Tree,
};
use crate::regexp_parser::lexer::{self, Token};

/// A `Regexp::Parser::Error` or scanner error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError;

type Result<T> = std::result::Result<T, ParseError>;

struct Parser {
    tree: Tree,
    node: ExprId,
    nesting: Vec<ExprId>,
    conditional_nesting: Vec<ExprId>,
    captured_group_counts: HashMap<ExprId, i64>,
    total_captured_group_count: i64,
}

/// `Regexp::Parser.parse(input, options:)`.
pub fn parse(input: &str, extended: bool, binary: bool) -> Result<Tree> {
    let tokens = lexer::lex(input, extended, binary).map_err(|_| ParseError)?;
    let mut root = Expression::new(Class::Root, "expression", "root", "", 0);
    root.parent = None;
    let mut parser = Parser {
        tree: Tree { exprs: vec![root] },
        node: Tree::ROOT,
        nesting: vec![Tree::ROOT],
        conditional_nesting: Vec::new(),
        captured_group_counts: HashMap::new(),
        total_captured_group_count: 0,
    };
    for token in tokens {
        parser.parse_token(token)?;
    }
    parser.assign_referenced_expressions()?;
    Ok(parser.tree)
}

fn class_name_of(token: &Token) -> Class {
    match token.ty {
        "free_space" => Class::FreeSpace,
        _ => Class::Terminal,
    }
}

impl Parser {
    fn get(&self, id: ExprId) -> &Expression {
        &self.tree.exprs[id]
    }

    fn get_mut(&mut self, id: ExprId) -> &mut Expression {
        &mut self.tree.exprs[id]
    }

    fn add(&mut self, mut exp: Expression) -> ExprId {
        let id = self.tree.exprs.len();
        exp.parent = None;
        self.tree.exprs.push(exp);
        id
    }

    fn from_token(class: Class, token: &Token) -> Expression {
        let mut exp = Expression::new(class, token.ty, &token.token, &token.text, token.ts);
        exp.level = token.level;
        exp.set_level = token.set_level;
        exp.conditional_level = token.conditional_level;
        exp
    }

    /// `node << exp`, with the `<<` of sequence operations and conditional
    /// expressions (they add to their last sequence).
    fn push_to(&mut self, target: ExprId, exp: ExprId) {
        let target = match self.get(target).class {
            Class::Alternation | Class::Intersection | Class::Conditional => {
                match self.get(target).expressions.last() {
                    Some(&last) => last,
                    None => target,
                }
            }
            _ => target,
        };
        self.get_mut(exp).parent = Some(target);
        self.get_mut(target).expressions.push(exp);
    }

    fn push(&mut self, exp: Expression) -> ExprId {
        let id = self.add(exp);
        self.push_to(self.node, id);
        id
    }

    fn parse_token(&mut self, token: Token) -> Result<()> {
        match token.ty {
            "anchor" | "escape" | "free_space" | "keep" | "literal" | "posixclass"
            | "nonposixclass" | "property" | "nonproperty" | "type" => {
                self.push(Self::from_token(class_name_of(&token), &token));
            }
            "meta" => match token.token.as_str() {
                "dot" => {
                    self.push(Self::from_token(Class::Terminal, &token));
                }
                "alternation" => self.sequence_operation(Class::Alternation, &token),
                _ => return Err(ParseError),
            },
            "assertion" | "group" => self.group(&token)?,
            "backref" => self.backref(&token)?,
            "conditional" => self.conditional(&token)?,
            "quantifier" => self.quantifier(&token)?,
            "set" => self.set(&token)?,
            _ => return Err(ParseError),
        }
        self.close_completed_character_set_range();
        Ok(())
    }

    fn group(&mut self, token: &Token) -> Result<()> {
        match token.token.as_str() {
            "options" | "options_switch" => {
                let exp = Self::from_token(Class::Group(GroupKind::Options), token);
                self.nest(exp);
            }
            "close" => self.decrease_nesting(),
            "comment" => {
                self.push(Self::from_token(Class::Group(GroupKind::Comment), token));
            }
            _ => self.open_group(token)?,
        }
        Ok(())
    }

    fn open_group(&mut self, token: &Token) -> Result<()> {
        let class = match token.token.as_str() {
            "absence" => Class::Group(GroupKind::Absence),
            "atomic" => Class::Group(GroupKind::Atomic),
            "capture" => Class::Group(GroupKind::Capture),
            "named" => Class::Group(GroupKind::Named),
            "passive" => Class::Group(GroupKind::Passive),
            "lookahead" | "lookbehind" | "nlookahead" | "nlookbehind" => Class::Assertion,
            _ => return Err(ParseError),
        };
        let mut group = Self::from_token(class, token);
        if class == Class::Group(GroupKind::Named) {
            let chars: Vec<char> = token.text.chars().collect();
            group.name = Some(chars[3..chars.len().saturating_sub(1)].iter().collect());
        }
        if group.is_capturing() {
            group.number = Some(self.total_captured_group_count + 1);
            *self.captured_group_counts.entry(self.node).or_insert(0) += 1;
            self.total_captured_group_count += 1;
        }
        self.nest(group);
        Ok(())
    }

    fn decrease_nesting(&mut self) {
        while self
            .nesting
            .last()
            .is_some_and(|&n| matches!(self.get(n).class, Class::Alternation | Class::Intersection))
        {
            self.nesting.pop();
            self.node = *self.nesting.last().unwrap_or(&Tree::ROOT);
        }
        self.nesting.pop();
        self.node = *self.nesting.last().unwrap_or(&Tree::ROOT);
        if let Some(&last) = self.get(self.node).expressions.last()
            && matches!(
                self.get(last).class,
                Class::Alternation | Class::Intersection
            )
        {
            self.node = last;
        }
    }

    fn backref(&mut self, token: &Token) -> Result<()> {
        let kind = match token.token.as_str() {
            "name_ref" => BackrefKind::Name,
            "name_recursion_ref" => BackrefKind::NameRecursionLevel,
            "name_call" => BackrefKind::NameCall,
            "number" | "number_ref" => BackrefKind::Number,
            "number_recursion_ref" => BackrefKind::NumberRecursionLevel,
            "number_call" => BackrefKind::NumberCall,
            "number_rel_ref" => BackrefKind::NumberRelative,
            "number_rel_call" => BackrefKind::NumberCallRelative,
            _ => return Err(ParseError),
        };
        let mut exp = Self::from_token(Class::Backreference(kind), token);
        let chars: Vec<char> = token.text.chars().collect();
        let inner: String = if chars.len() >= 4 {
            chars[3..chars.len() - 1].iter().collect()
        } else {
            String::new()
        };
        match kind {
            BackrefKind::Name | BackrefKind::NameCall => exp.name = Some(inner.clone()),
            BackrefKind::NameRecursionLevel => {
                let split = inner.find(['+', '-']).unwrap_or(inner.len());
                exp.name = Some(inner[..split].to_owned());
            }
            _ => {
                let number = crate::regex!(r"-?\d+")
                    .find(&token.text)
                    .and_then(|(s, e)| token.text[s..e].parse::<i64>().ok())
                    .unwrap_or(0);
                exp.number = Some(number);
            }
        }
        match kind {
            BackrefKind::NumberRecursionLevel => {
                let split = inner[1..].find(['+', '-']).map_or(inner.len(), |i| i + 1);
                let number: i64 = inner[..split].parse().unwrap_or(0);
                exp.number = Some(number);
                if crate::regex!(r"[<'][+-]").find(&token.text).is_some() {
                    self.assign_effective_number(&mut exp)?;
                } else {
                    exp.effective_number = Some(number);
                }
            }
            BackrefKind::NumberRelative | BackrefKind::NumberCallRelative => {
                self.assign_effective_number(&mut exp)?;
            }
            _ => {}
        }
        self.push(exp);
        Ok(())
    }

    fn assign_effective_number(&self, exp: &mut Expression) -> Result<()> {
        let number = exp.number.unwrap_or(0);
        let effective = number + self.total_captured_group_count + i64::from(number < 0);
        exp.effective_number = Some(effective);
        if effective > 0 {
            Ok(())
        } else {
            Err(ParseError)
        }
    }

    fn conditional(&mut self, token: &Token) -> Result<()> {
        match token.token.as_str() {
            "open" => {
                let exp = Self::from_token(Class::Conditional, token);
                let id = self.nest(exp);
                self.conditional_nesting.push(id);
            }
            "condition" => {
                let Some(&cond) = self.conditional_nesting.last() else {
                    return Err(ParseError);
                };
                let condition = self.add(Self::from_token(Class::Condition, token));
                self.get_mut(condition).parent = Some(cond);
                let expressions = &mut self.get_mut(cond).expressions;
                // `condition=`: replace any condition, at the front.
                expressions.insert(0, condition);
                self.add_sequence(cond, token.te)?;
            }
            "separator" => {
                let Some(&cond) = self.conditional_nesting.last() else {
                    return Err(ParseError);
                };
                self.add_sequence(cond, token.te)?;
                self.node = cond;
            }
            "close" => {
                self.conditional_nesting.pop();
                self.decrease_nesting();
                self.node = match self.conditional_nesting.last() {
                    Some(&c) => c,
                    None => *self.nesting.last().unwrap_or(&Tree::ROOT),
                };
            }
            _ => return Err(ParseError),
        }
        Ok(())
    }

    /// `add_sequence` of a conditional expression (a `Branch`) or a sequence
    /// operation.
    fn add_sequence(&mut self, owner: ExprId, ts: usize) -> Result<ExprId> {
        let owner_exp = self.get(owner);
        let (kind, conditional_level) = match owner_exp.class {
            Class::Conditional => {
                let branches = owner_exp
                    .expressions
                    .iter()
                    .filter(|&&e| matches!(self.get(e).class, Class::Sequence(_)))
                    .count();
                if branches == 2 {
                    return Err(ParseError);
                }
                (SequenceKind::Branch, owner_exp.conditional_level + 1)
            }
            Class::Intersection => (
                SequenceKind::IntersectedSequence,
                owner_exp.conditional_level,
            ),
            _ => (SequenceKind::Alternative, owner_exp.conditional_level),
        };
        let mut sequence = Expression::new(Class::Sequence(kind), "expression", "sequence", "", ts);
        sequence.level = owner_exp.level;
        sequence.set_level = owner_exp.set_level;
        sequence.conditional_level = conditional_level;
        let id = self.add(sequence);
        self.get_mut(id).parent = Some(owner);
        self.get_mut(owner).expressions.push(id);
        Ok(id)
    }

    fn nest(&mut self, exp: Expression) -> ExprId {
        let id = self.push(exp);
        self.nesting.push(id);
        self.node = id;
        id
    }

    fn sequence_operation(&mut self, class: Class, token: &Token) {
        if self.get(self.node).class != class {
            let operator = Self::from_token(class, token);
            let operator_id = self.add(operator);
            let sequence = self
                .add_sequence(operator_id, token.ts)
                .expect("a new operator has no sequences");
            // `sequence.expressions = node.expressions; node.expressions = []`
            let moved = std::mem::take(&mut self.get_mut(self.node).expressions);
            for &e in &moved {
                self.get_mut(e).parent = Some(sequence);
            }
            self.get_mut(sequence).expressions = moved;
            self.push_to(self.node, operator_id);
            self.nesting.push(operator_id);
            self.node = operator_id;
        }
        let node = self.node;
        let _ = self.add_sequence(node, token.te);
    }

    fn quantifier(&mut self, token: &Token) -> Result<()> {
        let mut target_node = self.extract_quantifier_target(self.node)?;
        if self.get(target_node).is_quantified() {
            let target = self.get(target_node);
            let mut new_group = Expression::new(
                Class::Group(GroupKind::Passive),
                "group",
                "passive",
                "",
                self.tree.ts(target_node),
            );
            new_group.level = target.level;
            new_group.set_level = target.set_level;
            new_group.conditional_level = target.conditional_level;
            new_group.implicit = true;
            let parent = target.parent.unwrap_or(self.node);
            let group = self.add(new_group);
            self.get_mut(group).parent = Some(parent);
            self.get_mut(group).expressions.push(target_node);
            self.get_mut(target_node).parent = Some(group);
            self.increment_group_level(target_node);
            if !self.get(target_node).is_terminal() {
                for exp in self.tree.each_expression(target_node, false) {
                    self.increment_group_level(exp);
                }
            }
            // `node.expressions[node.expressions.index(target_node)] = new_group`
            let siblings = &mut self.get_mut(parent).expressions;
            if let Some(index) = siblings.iter().position(|&e| e == target_node) {
                siblings[index] = group;
            }
            target_node = group;
        }

        if !crate::regex!(
            r"\A(?:zero_or_one|zero_or_more|one_or_more|interval)(?:_greedy|_reluctant|_possessive)?\z"
        )
        .find(&token.token)
        .is_some()
        {
            return Err(ParseError);
        }
        self.quantify(target_node, token)
    }

    /// `quantify`, with `Sequence#quantify` delegating to its target.
    fn quantify(&mut self, target: ExprId, token: &Token) -> Result<()> {
        match self.get(target).class {
            Class::Sequence(_) => {
                let inner = self.extract_quantifier_target(target)?;
                self.quantify(inner, token)
            }
            Class::FreeSpace => Err(ParseError),
            Class::Group(GroupKind::Options) if self.get(target).token == "options_switch" => {
                Err(ParseError)
            }
            _ => {
                self.get_mut(target).quantifier =
                    Some(Quantifier::new(&token.token, &token.text, token.ts));
                Ok(())
            }
        }
    }

    fn increment_group_level(&mut self, exp: ExprId) {
        self.get_mut(exp).level += 1;
    }

    /// `extract_quantifier_target`: the last non-decorative expression; the
    /// decorative ones after it become its pre-quantifier decorations.
    fn extract_quantifier_target(&mut self, node: ExprId) -> Result<ExprId> {
        let mut decorations = Vec::new();
        let expressions = self.get(node).expressions.clone();
        let mut target = None;
        for &exp in expressions.iter().rev() {
            if self.get(exp).is_decorative() {
                self.get_mut(exp).custom_to_s_handling = true;
                decorations.push(self.get(exp).text.clone());
                continue;
            }
            target = Some(exp);
            break;
        }
        let target = target.ok_or(ParseError)?;
        self.get_mut(target).pre_quantifier_decorations = decorations;
        Ok(target)
    }

    fn set(&mut self, token: &Token) -> Result<()> {
        match token.token.as_str() {
            "open" => {
                let mut exp = Self::from_token(Class::CharacterSet, token);
                exp.token = "character".to_owned();
                self.nest(exp);
            }
            "close" => {
                self.decrease_nesting();
                // `decrease_nesting(&:close)` closes the set that was popped;
                // sets are always closed by the time anyone looks.
            }
            "negate" => self.get_mut(self.node).negative = true,
            "range" => {
                let exp = Self::from_token(Class::Range, token);
                let last_is_intersected =
                    self.get(self.node).expressions.last().is_some_and(|&l| {
                        self.get(l).class == Class::Sequence(SequenceKind::IntersectedSequence)
                    });
                let scope = if last_is_intersected {
                    *self.get(self.node).expressions.last().unwrap_or(&self.node)
                } else {
                    self.node
                };
                let popped = self.get_mut(scope).expressions.pop();
                let range = self.nest(exp);
                if let Some(popped) = popped {
                    self.get_mut(popped).parent = Some(range);
                    self.get_mut(range).expressions.push(popped);
                }
            }
            "intersection" => self.sequence_operation(Class::Intersection, token),
            _ => return Err(ParseError),
        }
        Ok(())
    }

    fn close_completed_character_set_range(&mut self) {
        let node = self.get(self.node);
        if node.class == Class::Range && node.expressions.len() == 2 {
            self.decrease_nesting();
        }
    }

    /// `assign_referenced_expressions`: a reference to a group that doesn't
    /// exist is an error.
    fn assign_referenced_expressions(&self) -> Result<()> {
        let mut number_targets = vec![0_i64];
        let mut name_targets: Vec<&str> = Vec::new();
        let mut referrers = Vec::new();
        for id in self.tree.each_expression(Tree::ROOT, false) {
            let exp = self.get(id);
            if exp.is_referential() {
                referrers.push(id);
            } else if exp.class == Class::Group(GroupKind::Named) {
                name_targets.extend(exp.name.as_deref());
            } else if exp.class == Class::Group(GroupKind::Capture) {
                number_targets.extend(exp.number);
            }
        }
        for id in referrers {
            let exp = self.get(id);
            let ok = match exp.class {
                Class::Backreference(
                    BackrefKind::Name | BackrefKind::NameCall | BackrefKind::NameRecursionLevel,
                ) => exp
                    .name
                    .as_deref()
                    .is_some_and(|n| name_targets.contains(&n)),
                Class::Backreference(
                    BackrefKind::NumberRelative
                    | BackrefKind::NumberCallRelative
                    | BackrefKind::NumberRecursionLevel,
                ) => exp
                    .effective_number
                    .is_some_and(|n| number_targets.contains(&n)),
                Class::Backreference(_) => exp.number.is_some_and(|n| number_targets.contains(&n)),
                Class::Condition => {
                    let reference: String =
                        exp.text.chars().filter(|c| !"'<>()".contains(*c)).collect();
                    match reference.parse::<i64>() {
                        Ok(n) if !reference.contains(|c: char| !c.is_ascii_digit()) => {
                            number_targets.contains(&n)
                        }
                        _ => name_targets.contains(&reference.as_str()),
                    }
                }
                Class::Conditional => {
                    // `Conditional::Expression#reference` is its condition's.
                    exp.expressions.iter().any(|&e| {
                        let c = self.get(e);
                        if c.class != Class::Condition {
                            return false;
                        }
                        let reference: String =
                            c.text.chars().filter(|ch| !"'<>()".contains(*ch)).collect();
                        match reference.parse::<i64>() {
                            Ok(n) if !reference.contains(|ch: char| !ch.is_ascii_digit()) => {
                                number_targets.contains(&n)
                            }
                            _ => name_targets.contains(&reference.as_str()),
                        }
                    })
                }
                _ => true,
            };
            if !ok {
                return Err(ParseError);
            }
        }
        Ok(())
    }
}
