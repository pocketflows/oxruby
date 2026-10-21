//! Port of `lib/rubocop/cop/base.rb` (offense registration and the
//! autocorrect decision), `autocorrect_logic.rb` and `message_annotator.rb`.

use std::collections::HashSet;

use serde_yaml_ng::{Mapping, Value};

use crate::ast::{Node, NodeType};
use crate::comment_config::CommentConfig;
use crate::config::Config;
use crate::cop::offense::{Location, Status};
use crate::cop::{Corrector, Offense, Severity};
use crate::options::AutocorrectMode;
use crate::processed_source::{ProcessedSource, Range};

/// A cop. Ports implement the callbacks they need, like `on_send` in Ruby:
/// `on_node` receives every node whose type is in `node_types`, and
/// `after_node` every node whose type is in `after_node_types` (after its
/// children were visited).
pub trait Cop {
    /// The qualified cop name, e.g. `Layout/TrailingWhitespace`.
    fn name(&self) -> &'static str;

    /// Equivalent of `extend AutoCorrector`.
    fn support_autocorrect(&self) -> bool {
        false
    }

    /// `self.autocorrect_incompatible_with`: cop names whose corrections are
    /// skipped in the same iteration as this cop's.
    fn autocorrect_incompatible_with(&self) -> &'static [&'static str] {
        &[]
    }

    /// The node types `on_node` handles (`on_send`, `on_block`, ...).
    fn node_types(&self) -> &'static [NodeType] {
        &[]
    }

    /// The node types `after_node` handles (`after_send`, ...).
    fn after_node_types(&self) -> &'static [NodeType] {
        &[]
    }

    /// `RESTRICT_ON_SEND`: when not empty, `send`/`csend` callbacks only
    /// fire for these method names.
    fn restrict_on_send(&self) -> &'static [&'static str] {
        &[]
    }

    /// `minimum_target_ruby_version` (`TargetRubyVersion` support).
    fn minimum_target_ruby_version(&self) -> Option<f64> {
        None
    }

    /// `maximum_target_ruby_version`.
    fn maximum_target_ruby_version(&self) -> Option<f64> {
        None
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {}

    fn on_node<'a>(&mut self, _cx: &mut Investigation<'a, '_>, _node: Node<'a>) {}

    fn after_node<'a>(&mut self, _cx: &mut Investigation<'a, '_>, _node: Node<'a>) {}

    fn on_investigation_end(&mut self, _cx: &mut Investigation<'_, '_>) {}

    /// Called instead of the AST callbacks when the file has syntax errors.
    fn on_other_file(&mut self, _cx: &mut Investigation<'_, '_>) {}
}

pub fn department(cop_name: &str) -> &str {
    cop_name
        .rsplit_once('/')
        .map_or(cop_name, |(department, _)| department)
}

/// Options that affect how offenses are registered, derived from the CLI.
#[derive(Debug, Clone, Copy, Default)]
pub struct CopOptions {
    pub autocorrect: AutocorrectMode,
    pub display_cop_names: Option<bool>,
    pub json_format: bool,
    pub ignore_disable_comments: bool,
}

/// The state of one cop inspecting one file: what `Base` keeps in
/// `@current_offenses`, `@current_corrector` and friends, plus access to the
/// processed source and configuration.
pub struct Investigation<'a, 'src> {
    pub processed_source: &'a ProcessedSource<'src>,
    pub config: &'a Config,
    comment_config: &'a CommentConfig,
    cop_name: &'static str,
    cop_config: &'a Mapping,
    support_autocorrect: bool,
    options: CopOptions,
    offenses: Vec<Offense>,
    corrector: Option<Corrector>,
    offense_ranges: HashSet<Range>,
    /// Errors raised by the cop (RuboCop's `ErrorWithAnalyzedFileLocation`).
    pub errors: Vec<String>,
}

impl<'a, 'src> Investigation<'a, 'src> {
    pub fn new(
        cop: &dyn Cop,
        processed_source: &'a ProcessedSource<'src>,
        config: &'a Config,
        comment_config: &'a CommentConfig,
        options: CopOptions,
    ) -> Self {
        Self {
            processed_source,
            config,
            comment_config,
            cop_name: cop.name(),
            cop_config: config.for_badge(cop.name()),
            support_autocorrect: cop.support_autocorrect(),
            options,
            offenses: Vec::new(),
            corrector: processed_source
                .valid_syntax()
                .then(|| Corrector::new(processed_source.raw_source().len())),
            offense_ranges: HashSet::new(),
            errors: Vec::new(),
        }
    }

    pub fn cop_name(&self) -> &'static str {
        self.cop_name
    }

    pub fn cop_config(&self) -> &'a Mapping {
        self.cop_config
    }

    pub fn cop_config_value(&self, key: &str) -> Option<&'a Value> {
        self.cop_config.get(key)
    }

    pub fn cop_config_bool(&self, key: &str, default: bool) -> bool {
        self.cop_config
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    pub fn cop_config_str(&self, key: &str) -> Option<&'a str> {
        self.cop_config.get(key).and_then(Value::as_str)
    }

    pub fn cop_config_int(&self, key: &str) -> Option<i64> {
        self.cop_config.get(key).and_then(Value::as_i64)
    }

    /// A list parameter as strings (`Array(cop_config[key])`).
    pub fn cop_config_list(&self, key: &str) -> Vec<&'a str> {
        match self.cop_config.get(key) {
            Some(Value::Sequence(items)) => items.iter().filter_map(Value::as_str).collect(),
            Some(Value::String(s)) => vec![s.as_str()],
            _ => Vec::new(),
        }
    }

    pub fn target_ruby_version(&self) -> f64 {
        self.processed_source.ruby_version()
    }

    /// The source of a range.
    pub fn source(&self, range: Range) -> &'src str {
        self.processed_source.slice(range)
    }

    pub fn add_offense(&mut self, range: Range, message: &str) {
        self.register(range, message, None, None::<fn(&mut Corrector)>);
    }

    pub fn add_offense_with_severity(&mut self, range: Range, message: &str, severity: Severity) {
        self.register(range, message, Some(severity), None::<fn(&mut Corrector)>);
    }

    pub fn add_offense_with_correction(
        &mut self,
        range: Range,
        message: &str,
        correct: impl FnOnce(&mut Corrector),
    ) {
        self.register(range, message, None, Some(correct));
    }

    pub fn add_offense_full(
        &mut self,
        range: Range,
        message: &str,
        severity: Option<Severity>,
        correct: Option<impl FnOnce(&mut Corrector)>,
    ) {
        self.register(range, message, severity, correct);
    }

    fn register(
        &mut self,
        range: Range,
        message: &str,
        severity: Option<Severity>,
        correct: Option<impl FnOnce(&mut Corrector)>,
    ) {
        if !self.offense_ranges.insert(range) {
            return;
        }
        let severity = self.find_severity(severity);
        let message = self.annotate(message);

        let status = if self.enabled_lines(range) {
            let corrector = correct.and_then(|correct| {
                let mut corrector = Corrector::new(self.processed_source.raw_source().len());
                correct(&mut corrector);
                (!corrector.is_empty() || corrector.is_clobbered()).then_some(corrector)
            });
            if let Some(corrector) = &corrector {
                assert!(
                    self.support_autocorrect,
                    "The Cop {} must `extend AutoCorrector` to be able to autocorrect",
                    self.cop_name
                );
                if corrector.is_clobbered() {
                    // A ClobberingError raised by the cop itself: an error in
                    // the cop, the offense is not recorded.
                    self.errors.push(format!(
                        "An error occurred while {} cop was inspecting {}:{}:{}.",
                        self.cop_name,
                        self.processed_source.path().display(),
                        self.processed_source.line_of(range.begin_pos),
                        self.processed_source.column_of(range.begin_pos)
                    ));
                    return;
                }
            }
            match self.use_corrector(corrector) {
                Ok(status) => status,
                Err(()) => return,
            }
        } else {
            Status::Disabled
        };

        self.offenses.push(Offense {
            severity,
            location: Location::new(self.processed_source, range),
            message,
            cop_name: self.cop_name.to_owned(),
            status,
        });
    }

    fn use_corrector(&mut self, corrector: Option<Corrector>) -> Result<Status, ()> {
        if self.autocorrect() {
            let Some(corrector) = corrector else {
                return Ok(Status::Unsupported);
            };
            if let Some(current) = self.corrector.as_mut()
                && current.merge(&corrector).is_err()
            {
                self.errors.push(format!(
                    "An error occurred while {} cop was inspecting {}.",
                    self.cop_name,
                    self.processed_source.path().display()
                ));
                return Err(());
            }
            Ok(Status::Corrected)
        } else if corrector.is_some() && self.always_autocorrect() {
            Ok(Status::Uncorrected)
        } else {
            Ok(Status::Unsupported)
        }
    }

    /// `autocorrect?`
    pub fn autocorrect(&self) -> bool {
        self.options.autocorrect != AutocorrectMode::Off
            && self.support_autocorrect
            && self.autocorrect_enabled()
    }

    fn always_autocorrect(&self) -> bool {
        match self.cop_config.get("AutoCorrect") {
            None | Some(Value::Bool(true)) => true,
            Some(Value::String(s)) => s == "always" || s == "contextual",
            _ => false,
        }
    }

    pub fn safe_autocorrect(&self) -> bool {
        self.cop_config_bool("Safe", true) && self.cop_config_bool("SafeAutoCorrect", true)
    }

    fn autocorrect_enabled(&self) -> bool {
        match self.cop_config.get("AutoCorrect") {
            Some(Value::Bool(false)) => return false,
            Some(Value::String(s)) if s == "disabled" => return false,
            _ => {}
        }
        if self.options.autocorrect == AutocorrectMode::Safe {
            return self.safe_autocorrect();
        }
        true
    }

    fn enabled_lines(&self, range: Range) -> bool {
        if self.options.ignore_disable_comments {
            return true;
        }
        let first = self.processed_source.line_of(range.begin_pos);
        let last = self.processed_source.line_of(range.end_pos);
        self.comment_config
            .cop_enabled_at_lines(self.cop_name, first as i64, last as i64)
    }

    pub fn enabled_line(&self, line: usize) -> bool {
        self.options.ignore_disable_comments
            || self
                .comment_config
                .cop_enabled_at_lines(self.cop_name, line as i64, line as i64)
    }

    pub fn comment_config(&self) -> &'a CommentConfig {
        self.comment_config
    }

    fn find_severity(&self, severity: Option<Severity>) -> Severity {
        if self.cop_name == "Lint/Syntax" {
            return Severity::Fatal;
        }
        let custom = self
            .cop_config
            .get("Severity")
            .and_then(Value::as_str)
            .and_then(Severity::from_name);
        custom
            .or(severity)
            .unwrap_or(match department(self.cop_name) {
                "Lint" | "Security" => Severity::Warning,
                "Metrics" => Severity::Refactor,
                _ => Severity::Convention,
            })
    }

    /// Port of `MessageAnnotator#annotate`.
    fn annotate(&self, message: &str) -> String {
        let mut message = if self.display_cop_names() {
            format!("{}: {message}", self.cop_name)
        } else {
            message.to_owned()
        };
        if self.config.all_cops_bool("ExtraDetails", false)
            && let Some(details) = self.cop_config_str("Details")
            && !details.is_empty()
        {
            message = format!("{message} {details}");
        }
        message
    }

    fn display_cop_names(&self) -> bool {
        match self.options.display_cop_names {
            Some(display) => display,
            None if self.options.json_format => false,
            None => self.config.all_cops_bool("DisplayCopNames", true),
        }
    }

    pub fn into_parts(self) -> (Vec<Offense>, Option<Corrector>, Vec<String>) {
        (self.offenses, self.corrector, self.errors)
    }
}
