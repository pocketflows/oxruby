//! Port of `lib/rubocop/cop/team.rb` and the cop-selection parts of
//! `runner.rb` (`mobilized_cop_classes`).

use crate::config::Config;
use crate::cop::base::CopOptions;
use crate::cop::{Cop, Corrector, Investigation, Offense, registry};
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
}

pub struct Team<'a> {
    config: &'a Config,
    selection: &'a Selection,
    options: CopOptions,
}

impl<'a> Team<'a> {
    pub fn new(config: &'a Config, selection: &'a Selection, options: CopOptions) -> Self {
        Self {
            config,
            selection,
            options,
        }
    }

    fn mobilize_cops(&self, processed_source: &ProcessedSource<'_>) -> Vec<Box<dyn Cop>> {
        registry::all()
            .iter()
            .map(|new| new())
            .filter(|cop| {
                let name = cop.name();
                let selected = if self.selection.only.is_empty() {
                    self.config.cop_enabled(name)
                } else {
                    registry::matches(name, &self.selection.only)
                };
                selected
                    && !registry::matches(name, &self.selection.except)
                    && self.config.cop_relevant_file(name, processed_source.path())
            })
            .collect()
    }

    pub fn investigate(&self, processed_source: &ProcessedSource<'_>) -> InvestigationReport {
        let mut offenses = Vec::new();
        let mut corrector = Corrector::new();
        for mut cop in self.mobilize_cops(processed_source) {
            let mut cx =
                Investigation::new(cop.as_ref(), processed_source, self.config, self.options);
            if processed_source.valid_syntax() {
                cop.on_new_investigation(&mut cx);
            } else {
                cop.on_other_file(&mut cx);
            }
            let (cop_offenses, cop_corrector) = cx.into_parts();
            offenses.extend(cop_offenses);
            // Like `Team#collate_corrections`: a cop whose corrections clash
            // with an earlier cop's is skipped this round.
            if let Some(cop_corrector) = cop_corrector {
                let _ = corrector.merge(&cop_corrector);
            }
        }
        InvestigationReport {
            offenses,
            corrector: (!corrector.is_empty()).then_some(corrector),
        }
    }
}
