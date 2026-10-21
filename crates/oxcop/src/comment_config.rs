//! Port of `lib/rubocop/comment_config.rb` (with `comment_config/*.rb`):
//! which lines each cop is disabled on, from `# rubocop:disable` comments.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ast::Node;
use crate::config::Config;
use crate::directive_comment::{CopRegistry, DirectiveComment, STYLE_DISABLE_COPS_DIRECTIVE_COP};
use crate::processed_source::{ProcessedSource, Range};

pub const LINE_MIN: i64 = i64::MIN;
pub const LINE_MAX: i64 = i64::MAX;

/// `DirectiveRange`: an inclusive line range and the directive behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectiveRange {
    pub begin: i64,
    pub end: i64,
    /// Index into `CommentConfig::directives`; `None` for the injected
    /// config-disabled directive.
    pub directive: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct CopAnalysis {
    line_ranges: Vec<DirectiveRange>,
    start_line_number: Option<i64>,
    start_directive: Option<usize>,
}

impl CopAnalysis {
    fn close(&self, line: i64) -> Vec<DirectiveRange> {
        match self.start_line_number {
            Some(start) if line >= start => {
                let mut ranges = self.line_ranges.clone();
                ranges.push(DirectiveRange {
                    begin: start,
                    end: line,
                    directive: self.start_directive,
                });
                ranges
            }
            _ => self.line_ranges.clone(),
        }
    }
}

/// What `CommentConfig` derives for one file.
#[derive(Debug, Default)]
pub struct CommentConfig {
    pub cop_disabled_line_ranges: HashMap<String, Vec<DirectiveRange>>,
    pub opt_in_cops: HashSet<String>,
    /// Directive comments, by index (`DirectiveRange::directive`).
    pub directives: Vec<OwnedDirective>,
    /// `detached_next_directives`: `disable-next` and friends with no code
    /// to apply to.
    pub detached_next_directives: Vec<usize>,
}

/// The parts of a directive later consumers need.
#[derive(Debug, Clone)]
pub struct OwnedDirective {
    pub comment_range: Range,
    pub line_number: i64,
    pub reason: Option<String>,
}

impl CommentConfig {
    pub fn cop_enabled_at_lines(&self, cop: &str, first_line: i64, last_line: i64) -> bool {
        match self.cop_disabled_line_ranges.get(cop) {
            None => true,
            Some(ranges) => !ranges
                .iter()
                .any(|r| r.end >= first_line && r.begin <= last_line),
        }
    }

    pub fn cop_opted_in(&self, cop: &str) -> bool {
        self.opt_in_cops.contains(cop)
    }

    pub fn new(
        processed_source: &ProcessedSource<'_>,
        config: &Config,
        registry: &dyn CopRegistry,
    ) -> Self {
        if !processed_source.buffer_source().contains("rubocop") {
            return Self::default();
        }
        let mut analyzer = Analyzer {
            processed_source,
            config,
            registry,
            non_comment_token_lines: None,
            stack: Vec::new(),
            detached: Vec::new(),
            statement_bounds: HashMap::new(),
        };
        let directives: Vec<DirectiveComment<'_>> = processed_source
            .comments()
            .iter()
            .map(|c| {
                DirectiveComment::new(
                    c.text,
                    c.range,
                    processed_source.line_of(c.range.begin_pos) as i64,
                )
            })
            .filter(|d| d.has_match())
            .collect();
        let cop_disabled_line_ranges = analyzer.analyze(&directives);

        let mut opt_in_cops = HashSet::new();
        for directive in &directives {
            if directive.enabled() {
                if !directive.all_cops() {
                    opt_in_cops.extend(directive.raw_cop_names());
                }
            } else if directive.push() || directive.next() {
                for (op, names) in directive.signed_args() {
                    if op == '+' {
                        opt_in_cops.extend(names);
                    }
                }
            }
        }

        Self {
            cop_disabled_line_ranges,
            opt_in_cops,
            directives: directives
                .iter()
                .map(|d| OwnedDirective {
                    comment_range: d.comment_range,
                    line_number: d.line_number,
                    reason: d.reason(),
                })
                .collect(),
            detached_next_directives: analyzer.detached,
        }
    }

    /// `covering_disabled_range(range)&.directive&.reason`.
    pub fn suppression_reason(&self, cop: &str, first_line: i64, last_line: i64) -> Option<&str> {
        let range = self
            .cop_disabled_line_ranges
            .get(cop)?
            .iter()
            .find(|r| r.end >= first_line && r.begin <= last_line)?;
        self.directives[range.directive?].reason.as_deref()
    }
}

struct Analyzer<'a, 'src> {
    processed_source: &'a ProcessedSource<'src>,
    config: &'a Config,
    registry: &'a dyn CopRegistry,
    non_comment_token_lines: Option<BTreeSet<i64>>,
    stack: Vec<HashMap<String, CopAnalysis>>,
    detached: Vec<usize>,
    statement_bounds: HashMap<i64, (i64, i64)>,
}

impl Analyzer<'_, '_> {
    fn comment_only_line(&mut self, line: i64) -> bool {
        let tokens = self.non_comment_token_lines.get_or_insert_with(|| {
            let ps = self.processed_source;
            ps.tokens()
                .iter()
                .filter(|t| t.ty != "tCOMMENT")
                .map(|t| ps.line_of(t.range.begin_pos) as i64)
                .collect()
        });
        !tokens.contains(&line)
    }

    fn qualified_cop_name(&self, name: &str) -> String {
        let name = name.trim();
        if self.registry.names().iter().any(|n| n == name) || name.contains('/') {
            return name.to_owned();
        }
        let suffix = format!("/{name}");
        let candidates: Vec<&String> = self
            .registry
            .names()
            .iter()
            .filter(|n| n.ends_with(&suffix) && !n[..n.len() - suffix.len()].contains('/'))
            .collect();
        match candidates.as_slice() {
            [one] => (*one).clone(),
            _ => name.to_owned(),
        }
    }

    fn analyze(
        &mut self,
        directives: &[DirectiveComment<'_>],
    ) -> HashMap<String, Vec<DirectiveRange>> {
        let mut analyses: HashMap<String, CopAnalysis> = HashMap::new();

        // inject_disabled_cops_directives
        for cop_name in self.config.disabled_cop_names(self.registry.names()) {
            let analysis = analyses.entry(cop_name).or_default();
            // A synthetic `# rubocop:disable Cop` at line -infinity.
            *analysis = CopAnalysis {
                line_ranges: analysis.close(LINE_MIN),
                start_line_number: Some(LINE_MIN),
                start_directive: None,
            };
        }

        for (index, directive) in directives.iter().enumerate() {
            if directive.push() {
                self.stack.push(analyses.clone());
                let resolved = self.resolve_push_cops(directive);
                self.apply_push(&mut analyses, &resolved, directive, index);
            } else if directive.pop() {
                if !self.stack.is_empty() {
                    self.pop_state(&mut analyses, directive.line_number);
                }
            } else if directive.disable_next() {
                self.apply_disable_next(&mut analyses, directive, index);
            } else if directive.next() {
                let resolved = self.resolve_push_cops(directive);
                self.apply_next_directive(&mut analyses, &resolved, directive, index);
            } else if directive.enable_next() {
                self.apply_enable_next(&mut analyses, directive, index);
            } else {
                for cop_name in directive.cop_names(self.registry) {
                    let cop_name = self.qualified_cop_name(&cop_name);
                    let analysis = analyses.entry(cop_name.clone()).or_default().clone();
                    let new = self.analyze_cop(&analysis, directive, index);
                    analyses.insert(cop_name, new);
                }
            }
        }

        let style_directive_enabled = self
            .config
            .for_cop(STYLE_DISABLE_COPS_DIRECTIVE_COP)
            .get("Enabled")
            == Some(&serde_yaml_ng::Value::Bool(true));
        analyses
            .into_iter()
            .filter(|(cop, _)| {
                !(cop == STYLE_DISABLE_COPS_DIRECTIVE_COP && style_directive_enabled)
            })
            .map(|(cop, analysis)| (cop, analysis.close(LINE_MAX)))
            .collect()
    }

    fn analyze_cop(
        &mut self,
        analysis: &CopAnalysis,
        directive: &DirectiveComment<'_>,
        index: usize,
    ) -> CopAnalysis {
        let line = directive.line_number;
        if !self.comment_only_line(line) || directive.single_line() {
            // analyze_single_line
            if !directive.disabled() {
                analysis.clone()
            } else {
                let mut ranges = analysis.line_ranges.clone();
                ranges.push(DirectiveRange {
                    begin: line,
                    end: line,
                    directive: Some(index),
                });
                CopAnalysis {
                    line_ranges: ranges,
                    start_line_number: analysis.start_line_number,
                    start_directive: analysis.start_directive,
                }
            }
        } else if directive.disabled() {
            CopAnalysis {
                line_ranges: analysis.close(line),
                start_line_number: Some(line),
                start_directive: Some(index),
            }
        } else {
            CopAnalysis {
                line_ranges: analysis.close(line),
                start_line_number: None,
                start_directive: None,
            }
        }
    }

    // ---- push/pop --------------------------------------------------------------

    fn expand_cop_name(&self, name: &str) -> Vec<String> {
        let cops = if self.registry.is_department(name) {
            self.registry.names_for_department(name)
        } else {
            vec![name.to_owned()]
        };
        cops.iter().map(|c| self.qualified_cop_name(c)).collect()
    }

    fn resolve_push_cops(&self, directive: &DirectiveComment<'_>) -> Vec<(char, Vec<String>)> {
        directive
            .signed_args()
            .into_iter()
            .map(|(op, names)| {
                (
                    op,
                    names.iter().flat_map(|n| self.expand_cop_name(n)).collect(),
                )
            })
            .collect()
    }

    fn apply_push(
        &self,
        analyses: &mut HashMap<String, CopAnalysis>,
        resolved: &[(char, Vec<String>)],
        directive: &DirectiveComment<'_>,
        index: usize,
    ) {
        let line = directive.line_number;
        for (op, cops) in resolved {
            for cop in cops {
                let analysis = analyses.entry(cop.clone()).or_default();
                if *op == '-' && analysis.start_line_number.is_none() {
                    *analysis = CopAnalysis {
                        line_ranges: analysis.line_ranges.clone(),
                        start_line_number: Some(line),
                        start_directive: Some(index),
                    };
                } else if *op == '+' && analysis.start_line_number.is_some() {
                    *analysis = CopAnalysis {
                        line_ranges: analysis.close(line),
                        start_line_number: None,
                        start_directive: None,
                    };
                }
            }
        }
    }

    fn pop_state(&mut self, analyses: &mut HashMap<String, CopAnalysis>, line: i64) {
        let restore_point = self.stack.pop().unwrap();
        let keys: Vec<String> = restore_point
            .keys()
            .chain(analyses.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for cop in keys {
            let current = analyses.get(&cop).cloned().unwrap_or_default();
            let ranges = current.close(line - 1);
            let restored = restore_point.get(&cop);
            let new = match restored.and_then(|r| r.start_line_number.map(|_| r)) {
                None => CopAnalysis {
                    line_ranges: ranges,
                    start_line_number: None,
                    start_directive: None,
                },
                Some(restored) => CopAnalysis {
                    line_ranges: ranges,
                    start_line_number: Some(line),
                    start_directive: restored.start_directive,
                },
            };
            analyses.insert(cop, new);
        }
    }

    // ---- disable-next ------------------------------------------------------------

    fn statement_scope_after(&mut self, line: i64) -> Option<(i64, i64)> {
        if !self.comment_only_line(line) {
            return None;
        }
        let code_line = self.attached_code_line(line)?;
        if let Some(bounds) = self.statement_bounds.get(&code_line) {
            return Some(*bounds);
        }
        let bounds = self.statement_bounds_at(code_line);
        self.statement_bounds.insert(code_line, bounds);
        Some(bounds)
    }

    fn attached_code_line(&mut self, directive_line: i64) -> Option<i64> {
        let line_count = self.processed_source.lines().len() as i64;
        for line in (directive_line + 1)..=line_count {
            if !self.comment_only_line(line) {
                return Some(line);
            }
            if self.processed_source.lines()[(line - 1) as usize]
                .trim()
                .is_empty()
            {
                return None;
            }
        }
        None
    }

    fn statement_bounds_at(&self, line: i64) -> (i64, i64) {
        match self.statement_starting_at(line) {
            None => (line, line),
            Some(statement) => (line, self.statement_end_line(statement)),
        }
    }

    fn first_line(&self, node: Node<'_>) -> i64 {
        self.processed_source.line_of(node.source_range().begin_pos) as i64
    }

    fn last_line(&self, node: Node<'_>) -> i64 {
        self.processed_source.line_of(node.source_range().end_pos) as i64
    }

    fn statement_starting_at(&self, line: i64) -> Option<Node<'_>> {
        let ast = self.processed_source.ast()?;
        let mut best: Option<(i64, Node<'_>)> = None;
        for node in std::iter::once(ast).chain(ast.descendants()) {
            if node.loc().expression.is_some()
                && self.first_line(node) == line
                && node.ty() != crate::ast::NodeType::Begin
            {
                let last = self.last_line(node);
                if best.is_none_or(|(l, _)| last > l) {
                    best = Some((last, node));
                }
            }
        }
        best.map(|(_, n)| n)
    }

    fn statement_end_line(&self, statement: Node<'_>) -> i64 {
        std::iter::once(statement)
            .chain(statement.descendants())
            .filter(|n| n.loc().expression.is_some())
            .map(|n| match n.loc().heredoc_end {
                Some(end) if n.loc().kind == crate::ast::MapKind::Heredoc => {
                    self.processed_source.line_of(end.begin_pos) as i64
                }
                _ => self.last_line(n),
            })
            .max()
            .unwrap_or(0)
    }

    fn add_next_range(
        analyses: &mut HashMap<String, CopAnalysis>,
        cop_name: String,
        range: DirectiveRange,
    ) {
        analyses
            .entry(cop_name)
            .or_default()
            .line_ranges
            .push(range);
    }

    fn apply_disable_next(
        &mut self,
        analyses: &mut HashMap<String, CopAnalysis>,
        directive: &DirectiveComment<'_>,
        index: usize,
    ) {
        let Some((begin, end)) = self.statement_scope_after(directive.line_number) else {
            self.detached.push(index);
            return;
        };
        let range = DirectiveRange {
            begin,
            end,
            directive: Some(index),
        };
        for cop_name in directive.cop_names(self.registry) {
            Self::add_next_range(analyses, self.qualified_cop_name(&cop_name), range);
        }
    }

    fn apply_next_directive(
        &mut self,
        analyses: &mut HashMap<String, CopAnalysis>,
        resolved: &[(char, Vec<String>)],
        directive: &DirectiveComment<'_>,
        index: usize,
    ) {
        let Some(bounds) = self.statement_scope_after(directive.line_number) else {
            self.detached.push(index);
            return;
        };
        let range = DirectiveRange {
            begin: bounds.0,
            end: bounds.1,
            directive: Some(index),
        };
        for (op, cops) in resolved {
            for cop in cops {
                if *op == '-' {
                    Self::add_next_range(analyses, cop.clone(), range);
                } else {
                    Self::suspend_disable(analyses, cop, bounds);
                }
            }
        }
    }

    fn apply_enable_next(
        &mut self,
        analyses: &mut HashMap<String, CopAnalysis>,
        directive: &DirectiveComment<'_>,
        index: usize,
    ) {
        let Some(bounds) = self.statement_scope_after(directive.line_number) else {
            self.detached.push(index);
            return;
        };
        for cop_name in directive.cop_names(self.registry) {
            let cop = self.qualified_cop_name(&cop_name);
            Self::suspend_disable(analyses, &cop, bounds);
        }
    }

    fn suspend_disable(analyses: &mut HashMap<String, CopAnalysis>, cop: &str, bounds: (i64, i64)) {
        let analysis = analyses.entry(cop.to_owned()).or_default();
        if analysis.start_line_number.is_none() {
            return;
        }
        *analysis = CopAnalysis {
            line_ranges: analysis.close(bounds.0 - 1),
            start_line_number: Some(bounds.1 + 1),
            start_directive: analysis.start_directive,
        };
    }
}
