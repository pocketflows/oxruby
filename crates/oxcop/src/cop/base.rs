//! Port of `lib/rubocop/cop/base.rb` (offense registration and the
//! autocorrect decision) and `message_annotator.rb`.

use std::collections::HashSet;

use serde_yaml_ng::{Mapping, Value};

use crate::config::Config;
use crate::cop::offense::{Location, Status};
use crate::cop::{Corrector, Offense, Severity};
use crate::options::AutocorrectMode;
use crate::processed_source::{ProcessedSource, Range};

pub trait Cop {
    /// The qualified cop name, e.g. `Layout/TrailingWhitespace`.
    fn name(&self) -> &'static str;

    /// Equivalent of `extend AutoCorrector`.
    fn support_autocorrect(&self) -> bool {
        false
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {}

    /// Called instead of the AST callbacks when the file has syntax errors.
    fn on_other_file(&mut self, _cx: &mut Investigation<'_, '_>) {}
}

pub fn department(cop_name: &str) -> &str {
    cop_name
        .split_once('/')
        .map_or(cop_name, |(department, _)| department)
}

/// Options that affect how offenses are registered, derived from the CLI.
#[derive(Debug, Clone, Copy, Default)]
pub struct CopOptions {
    pub autocorrect: AutocorrectMode,
    pub display_cop_names: Option<bool>,
    pub json_format: bool,
}

/// The state of one cop inspecting one file: what `Base` keeps in
/// `@current_offenses`, `@current_corrector` and friends.
pub struct Investigation<'a, 'src> {
    pub processed_source: &'a ProcessedSource<'src>,
    pub config: &'a Config,
    cop_name: &'static str,
    support_autocorrect: bool,
    options: CopOptions,
    offenses: Vec<Offense>,
    corrector: Option<Corrector>,
    offense_ranges: HashSet<Range>,
}

impl<'a, 'src> Investigation<'a, 'src> {
    pub fn new(
        cop: &dyn Cop,
        processed_source: &'a ProcessedSource<'src>,
        config: &'a Config,
        options: CopOptions,
    ) -> Self {
        Self {
            processed_source,
            config,
            cop_name: cop.name(),
            support_autocorrect: cop.support_autocorrect(),
            options,
            offenses: Vec::new(),
            corrector: processed_source.valid_syntax().then(Corrector::new),
            offense_ranges: HashSet::new(),
        }
    }

    pub fn cop_name(&self) -> &'static str {
        self.cop_name
    }

    pub fn cop_config(&self) -> &'a Mapping {
        self.config.for_cop(self.cop_name)
    }

    pub fn cop_config_bool(&self, key: &str, default: bool) -> bool {
        self.cop_config()
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(default)
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

        let corrector = correct.and_then(|correct| {
            let mut corrector = Corrector::new();
            correct(&mut corrector);
            assert!(
                corrector.is_empty() || self.support_autocorrect,
                "The Cop {} must `extend AutoCorrector` to be able to autocorrect",
                self.cop_name
            );
            (!corrector.is_empty()).then_some(corrector)
        });
        let status = self.use_corrector(corrector);

        self.offenses.push(Offense {
            severity,
            location: Location::new(self.processed_source, range),
            message,
            cop_name: self.cop_name.to_owned(),
            status,
        });
    }

    fn use_corrector(&mut self, corrector: Option<Corrector>) -> Status {
        if self.autocorrect() {
            let Some(corrector) = corrector else {
                return Status::Unsupported;
            };
            match self
                .corrector
                .as_mut()
                .map(|current| current.merge(&corrector))
            {
                Some(Ok(())) => Status::Corrected,
                _ => Status::Uncorrected,
            }
        } else if corrector.is_some() && self.processed_source.valid_syntax() {
            Status::Uncorrected
        } else {
            Status::Unsupported
        }
    }

    fn autocorrect(&self) -> bool {
        self.options.autocorrect != AutocorrectMode::Off
            && self.support_autocorrect
            && self.autocorrect_enabled()
    }

    fn autocorrect_enabled(&self) -> bool {
        match self.cop_config().get("AutoCorrect") {
            Some(Value::Bool(false)) => return false,
            Some(Value::String(s)) if s == "disabled" => return false,
            _ => {}
        }
        if self.options.autocorrect == AutocorrectMode::Safe {
            return self.cop_config_bool("Safe", true)
                && self.cop_config_bool("SafeAutoCorrect", true);
        }
        true
    }

    fn find_severity(&self, severity: Option<Severity>) -> Severity {
        if self.cop_name == "Lint/Syntax" {
            return Severity::Fatal;
        }
        let custom = self
            .cop_config()
            .get("Severity")
            .and_then(Value::as_str)
            .and_then(Severity::from_name);
        custom
            .or(severity)
            .unwrap_or(if department(self.cop_name) == "Lint" {
                Severity::Warning
            } else {
                Severity::Convention
            })
    }

    /// Port of `MessageAnnotator#annotate` (cop names only so far).
    fn annotate(&self, message: &str) -> String {
        if self.display_cop_names() {
            format!("{}: {message}", self.cop_name)
        } else {
            message.to_owned()
        }
    }

    fn display_cop_names(&self) -> bool {
        match self.options.display_cop_names {
            Some(display) => display,
            None if self.options.json_format => false,
            None => self.config.all_cops_bool("DisplayCopNames", true),
        }
    }

    pub fn into_parts(self) -> (Vec<Offense>, Option<Corrector>) {
        (self.offenses, self.corrector)
    }
}
