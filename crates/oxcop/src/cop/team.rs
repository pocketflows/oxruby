//! Port of `lib/rubocop/cop/team.rb` and the cop-selection parts of
//! `runner.rb` (`mobilized_cop_classes`).

use crate::comment_config::CommentConfig;
use crate::config::Config;
use crate::cop::base::{AutoGenState, CopError, CopOptions, OffenseToCheck};
use crate::cop::commissioner::{self, CopReport};
use crate::cop::{Cop, Corrector, Investigation, Offense, registry};
use crate::options::AutocorrectMode;
use crate::processed_source::ProcessedSource;

#[derive(Debug, Clone, Default)]
pub struct Selection {
    pub only: Vec<String>,
    pub except: Vec<String>,
    /// `--raw-config` (RuboCop's specs): `processed_source.registry` is the
    /// cops the configuration names plus the cop under test, as
    /// `CopHelper#registry` builds it.
    pub registry_from_config: bool,
}

pub struct InvestigationReport {
    pub offenses: Vec<Offense>,
    /// The merged corrections of every cop, when any were made.
    pub corrector: Option<Corrector>,
    pub errors: Vec<String>,
    /// What cops raised (for the spec shim).
    pub raised: Vec<CopError>,
    /// The team's `--auto-gen-config` state after this investigation (see
    /// `Team::auto_gen`).
    pub auto_gen: Vec<(String, AutoGenState)>,
    pub comment_config: CommentConfig,
    /// A cop's `validate_config` failed: nothing was investigated.
    pub validation_error: Option<CopError>,
}

pub struct Team<'a> {
    config: &'a Config,
    selection: &'a Selection,
    options: CopOptions,
    /// `--persist-cops`: reuse the cops mobilized for the first source.
    pub persist_cops: bool,
    persisted: std::cell::RefCell<Option<Vec<Box<dyn Cop>>>>,
    /// Each cop's `--auto-gen-config` state. Upstream it lives in class-level
    /// hashes, so it carries over from one investigation to the next (the
    /// autocorrect loop, and in specs from `expect_offense` to
    /// `expect_correction`). Exclude limits are the values set so far.
    pub auto_gen: std::cell::RefCell<std::collections::BTreeMap<String, AutoGenState>>,
    /// Given to each mobilized cop (`set_offenses_to_check`).
    pub offenses_to_check: Option<Vec<OffenseToCheck>>,
}

impl<'a> Team<'a> {
    pub fn new(config: &'a Config, selection: &'a Selection, options: CopOptions) -> Self {
        Self {
            config,
            selection,
            options,
            persist_cops: false,
            persisted: std::cell::RefCell::new(None),
            auto_gen: std::cell::RefCell::default(),
            offenses_to_check: None,
        }
    }

    /// `Runner#mobilize_cop_badge?`, then `roundup_relevant_cops`: `--only`
    /// and `--except` take departments, but only a cop named in `--only`
    /// itself runs when the config disables it (`Registry#enabled_cop_name?`).
    fn selected(&self, name: &str, comment_config: &CommentConfig) -> bool {
        if registry::matches(name, &self.selection.except) {
            return false;
        }
        if !self.selection.only.is_empty() && !registry::matches(name, &self.selection.only) {
            return false;
        }
        comment_config.cop_opted_in(name)
            || self.selection.only.iter().any(|only| only == name)
            || self.config.cop_enabled(name)
    }

    /// A team of exactly these cops (`Team.mobilize([...])`), kept for every
    /// investigation.
    pub fn with_cops(
        config: &'a Config,
        selection: &'a Selection,
        options: CopOptions,
        cops: Vec<Box<dyn Cop>>,
    ) -> Self {
        let team = Self::new(config, selection, options);
        *team.persisted.borrow_mut() = Some(cops);
        Self {
            persist_cops: true,
            ..team
        }
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
            .map(|new| {
                let mut cop = new();
                if let Some(offenses) = &self.offenses_to_check {
                    cop.set_offenses_to_check(offenses.clone());
                }
                cop
            })
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
        let registry = crate::config::SelectedCops {
            known: self.config.known_cops(),
            only: &self.selection.only,
            except: &self.selection.except,
            config: self.selection.registry_from_config.then_some(self.config),
        };
        let comment_config = CommentConfig::new(processed_source, self.config, &registry);
        let persisted = self.persisted.borrow_mut().take();
        let cops = match persisted {
            Some(cops) => cops,
            None => self.mobilize_cops(processed_source, &comment_config),
        };
        // `Team#validate_config`, which upstream runs when the team is set up.
        for cop in &cops {
            let cx = Investigation::new(
                cop.as_ref(),
                processed_source,
                self.config,
                &comment_config,
                self.options,
            );
            if let Err(message) = cop.validate_config(&cx) {
                return InvestigationReport {
                    offenses: Vec::new(),
                    corrector: None,
                    errors: Vec::new(),
                    raised: Vec::new(),
                    auto_gen: Vec::new(),
                    comment_config,
                    validation_error: Some(CopError {
                        class: "RuboCop::ValidationError".to_owned(),
                        message,
                    }),
                };
            }
        }
        let (mut autocorrect_cops, mut other_cops): (Vec<_>, Vec<_>) = cops
            .into_iter()
            .partition(|cop| self.autocorrects(cop.as_ref()));

        let seed = self.auto_gen.borrow().clone();
        let mut reports = commissioner::investigate(
            &mut autocorrect_cops,
            processed_source,
            self.config,
            &comment_config,
            self.options,
            &seed,
        );
        let corrector = collate_corrections(&reports, processed_source);
        if corrector.is_none() {
            reports.extend(commissioner::investigate(
                &mut other_cops,
                processed_source,
                self.config,
                &comment_config,
                self.options,
                &seed,
            ));
        }

        if self.persist_cops {
            let mut cops = autocorrect_cops;
            cops.extend(other_cops);
            *self.persisted.borrow_mut() = Some(cops);
        }

        let mut offenses = Vec::new();
        let mut errors = Vec::new();
        let mut raised = Vec::new();
        let mut state = self.auto_gen.borrow_mut();
        for report in reports {
            offenses.extend(report.offenses);
            errors.extend(report.errors);
            raised.extend(report.raised);
            if !report.auto_gen.is_empty() {
                let entry = state.entry(report.cop_name.to_owned()).or_default();
                let mut limits = std::mem::take(&mut entry.exclude_limits);
                limits.extend(report.auto_gen.exclude_limits);
                *entry = AutoGenState {
                    exclude_limits: limits,
                    ..report.auto_gen
                };
            }
        }
        let auto_gen = state
            .iter()
            .map(|(cop, state)| (cop.clone(), state.clone()))
            .collect();
        drop(state);
        InvestigationReport {
            offenses,
            corrector,
            errors,
            raised,
            auto_gen,
            comment_config,
            validation_error: None,
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
