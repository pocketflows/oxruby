//! Port of `lib/rubocop/cop/team.rb` and the cop-selection parts of
//! `runner.rb` (`mobilized_cop_classes`).

use crate::comment_config::CommentConfig;
use crate::config::Config;
use crate::cop::base::CopOptions;
use crate::cop::commissioner::{self, CopReport};
use crate::cop::{Cop, Corrector, Offense, registry};
use crate::options::AutocorrectMode;
use crate::processed_source::ProcessedSource;

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub only: Vec<String>,
    pub except: Vec<String>,
}

pub struct InvestigationReport {
    pub offenses: Vec<Offense>,
    /// The merged corrections of every cop, when any were made.
    pub corrector: Option<Corrector>,
    pub errors: Vec<String>,
    pub comment_config: CommentConfig,
}

pub struct Team<'a> {
    config: &'a Config,
    selection: &'a Selection,
    options: CopOptions,
    /// `--persist-cops`: reuse the cops mobilized for the first source.
    pub persist_cops: bool,
    persisted: std::cell::RefCell<Option<Vec<Box<dyn Cop>>>>,
}

impl<'a> Team<'a> {
    pub fn new(config: &'a Config, selection: &'a Selection, options: CopOptions) -> Self {
        Self {
            config,
            selection,
            options,
            persist_cops: false,
            persisted: std::cell::RefCell::new(None),
        }
    }

    fn selected(&self, name: &str, comment_config: &CommentConfig) -> bool {
        if registry::matches(name, &self.selection.except) {
            return false;
        }
        if !self.selection.only.is_empty() {
            return registry::matches(name, &self.selection.only);
        }
        self.config.cop_enabled(name) || comment_config.cop_opted_in(name)
    }

    /// `roundup_relevant_cops`.
    fn mobilize_cops(
        &self,
        processed_source: &ProcessedSource<'_>,
        comment_config: &CommentConfig,
    ) -> Vec<Box<dyn Cop>> {
        let target = processed_source.ruby_version();
        registry::all()
            .iter()
            .map(|new| new())
            .filter(|cop| {
                let name = cop.name();
                self.selected(name, comment_config)
                    && self.config.cop_relevant_file(name, processed_source.path())
                    && cop
                        .minimum_target_ruby_version()
                        .is_none_or(|min| target >= min)
                    && cop
                        .maximum_target_ruby_version()
                        .is_none_or(|max| target <= max)
            })
            .collect()
    }

    /// `cop.autocorrect?`, used to run correcting cops first.
    fn autocorrects(&self, cop: &dyn Cop) -> bool {
        if self.options.autocorrect == AutocorrectMode::Off || !cop.support_autocorrect() {
            return false;
        }
        let cfg = self.config.for_badge(cop.name());
        match cfg.get("AutoCorrect") {
            Some(serde_yaml_ng::Value::Bool(false)) => return false,
            Some(serde_yaml_ng::Value::String(s)) if s == "disabled" => return false,
            _ => {}
        }
        if self.options.autocorrect == AutocorrectMode::Safe {
            let flag = |key: &str| {
                cfg.get(key)
                    .and_then(serde_yaml_ng::Value::as_bool)
                    .unwrap_or(true)
            };
            return flag("Safe") && flag("SafeAutoCorrect");
        }
        true
    }

    pub fn investigate(&self, processed_source: &ProcessedSource<'_>) -> InvestigationReport {
        let comment_config =
            CommentConfig::new(processed_source, self.config, self.config.known_cops());
        let persisted = self.persisted.borrow_mut().take();
        let cops = match persisted {
            Some(cops) => cops,
            None => self.mobilize_cops(processed_source, &comment_config),
        };
        let (mut autocorrect_cops, mut other_cops): (Vec<_>, Vec<_>) = cops
            .into_iter()
            .partition(|cop| self.autocorrects(cop.as_ref()));

        let mut reports = commissioner::investigate(
            &mut autocorrect_cops,
            processed_source,
            self.config,
            &comment_config,
            self.options,
        );
        let corrector = collate_corrections(&reports, processed_source);
        if corrector.is_none() {
            reports.extend(commissioner::investigate(
                &mut other_cops,
                processed_source,
                self.config,
                &comment_config,
                self.options,
            ));
        }

        if self.persist_cops {
            let mut cops = autocorrect_cops;
            cops.extend(other_cops);
            *self.persisted.borrow_mut() = Some(cops);
        }

        let mut offenses = Vec::new();
        let mut errors = Vec::new();
        for report in reports {
            offenses.extend(report.offenses);
            errors.extend(report.errors);
        }
        InvestigationReport {
            offenses,
            corrector,
            errors,
            comment_config,
        }
    }
}

/// `collate_corrections`: merges each cop's corrector, skipping cops whose
/// corrections clash with earlier ones and cops another cop is
/// `autocorrect_incompatible_with`.
fn collate_corrections(
    reports: &[CopReport],
    processed_source: &ProcessedSource<'_>,
) -> Option<Corrector> {
    let mut corrector = Corrector::new(processed_source.buffer_source().len());
    let mut skips: Vec<&str> = Vec::new();
    for report in reports {
        let Some(cop_corrector) = &report.corrector else {
            continue;
        };
        if cop_corrector.is_empty() || skips.contains(&report.cop_name) {
            continue;
        }
        // suppress_clobbering: a clashing corrector is skipped.
        let _ = corrector.merge(cop_corrector);
        skips.extend(report.incompatible_with.iter().copied());
    }
    (!corrector.is_empty()).then_some(corrector)
}
