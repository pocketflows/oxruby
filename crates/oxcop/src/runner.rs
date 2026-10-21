//! Port of `lib/rubocop/runner.rb`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_yaml_ng::Value;

use crate::config::Config;
use crate::config_loader::{ConfigError, ConfigStore};
use crate::cop::base::CopOptions;
use crate::cop::team::{Selection, Team};
use crate::cop::{Offense, Severity};
use crate::formatter::Formatter;
use crate::options::{AutocorrectMode, Options};
use crate::path_util;
use crate::processed_source::ProcessedSource;
use crate::target_finder::TargetFinder;
use crate::target_ruby::target_ruby_version;

pub const MAX_ITERATIONS: usize = 200;

pub struct Runner<'a> {
    options: &'a Options,
    config_store: ConfigStore,
    selection: Selection,
    cop_options: CopOptions,
    /// With `--stdin` and autocorrect, the corrected source to echo back.
    pub corrected_stdin: Option<String>,
    pub errors: Vec<String>,
}

impl<'a> Runner<'a> {
    pub fn new(options: &'a Options, cop_options: CopOptions) -> Self {
        let mut config_store = ConfigStore::new(options.config.clone());
        config_store.raw = options.raw_config;
        Self {
            options,
            config_store,
            selection: Selection {
                only: options.only.clone(),
                except: options.except.clone(),
            },
            cop_options,
            corrected_stdin: None,
            errors: Vec::new(),
        }
    }

    pub fn find_target_files(&mut self) -> Result<Vec<PathBuf>, ConfigError> {
        if let Some(stdin_path) = &self.options.stdin {
            return Ok(vec![path_util::expand_path(stdin_path)]);
        }
        TargetFinder::new(&mut self.config_store, self.options.force_exclusion)
            .find(&self.options.paths)
    }

    /// Returns whether all files passed (no offense at or above `FailLevel`).
    pub fn run(
        &mut self,
        formatter: &mut dyn Formatter,
        stdin: &mut dyn Read,
    ) -> Result<bool, ConfigError> {
        let files = self.find_target_files()?;
        let stdin_source = match self.options.stdin {
            Some(_) => {
                let mut source = String::new();
                stdin
                    .read_to_string(&mut source)
                    .map_err(|e| ConfigError(format!("reading stdin: {e}")))?;
                Some(source)
            }
            None => None,
        };

        formatter.started(&files);
        let mut all_passed = true;
        let mut inspected = Vec::new();
        for file in &files {
            formatter.file_started(file);
            let offenses = self.file_offenses(file, stdin_source.as_deref())?;
            let fail_level = self.minimum_severity_to_fail(file)?;
            if offenses
                .iter()
                .any(|offense| !offense.corrected() && offense.severity >= fail_level)
            {
                all_passed = false;
            }
            formatter.file_finished(file, &offenses);
            inspected.push(file.clone());
        }
        formatter.finished(&inspected);
        Ok(all_passed)
    }

    fn minimum_severity_to_fail(&mut self, file: &Path) -> Result<Severity, ConfigError> {
        let config = self.config_store.for_file(file)?;
        Ok(config
            .for_all_cops()
            .get("FailLevel")
            .and_then(Value::as_str)
            .and_then(Severity::from_name)
            .unwrap_or(Severity::Refactor))
    }

    fn file_offenses(
        &mut self,
        file: &Path,
        stdin_source: Option<&str>,
    ) -> Result<Vec<Offense>, ConfigError> {
        let source = match stdin_source {
            Some(source) => source.to_owned(),
            None => match std::fs::read(file) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(e) => {
                    self.errors.push(format!("{}: {e}", file.display()));
                    return Ok(Vec::new());
                }
            },
        };
        let config = self.config_store.for_file(file)?;
        let (corrected_source, offenses) = self.do_inspection_loop(file, source, &config);

        if let Some(corrected) = corrected_source {
            if stdin_source.is_some() {
                self.corrected_stdin = Some(corrected);
            } else if let Err(e) = std::fs::write(file, corrected) {
                self.errors.push(format!("{}: {e}", file.display()));
            }
        } else if stdin_source.is_some() {
            self.corrected_stdin = Some(source_for_stdin(stdin_source));
        }

        let mut offenses: Vec<Offense> = offenses.into_iter().flatten().collect();
        offenses.retain(|offense| !offense.disabled());
        offenses.sort();
        // The runner's `offenses_by_iteration.flatten.uniq` merges offenses
        // with the same line, column and message. RuboCop's specs look at the
        // cop's own offenses, before that, so the spec shim keeps them all.
        if !self.options.raw_config {
            offenses.dedup();
        }
        Ok(offenses)
    }

    /// Inspects (and with autocorrect, re-inspects) until no more corrections
    /// are made. Returns the corrected source, if any, and the offenses found
    /// in each iteration.
    fn do_inspection_loop(
        &self,
        file: &Path,
        mut source: String,
        config: &Config,
    ) -> (Option<String>, Vec<Vec<Offense>>) {
        let ruby_version = target_ruby_version(config);
        let mut team = Team::new(config, &self.selection, self.cop_options);
        team.persist_cops = self.options.persist_cops;
        let mut offenses_by_iteration: Vec<Vec<Offense>> = Vec::new();
        let mut seen_sources: Vec<String> = Vec::new();
        let mut corrected = false;

        let max_iterations = if self.options.autocorrect_once {
            1
        } else {
            MAX_ITERATIONS
        };
        for _ in 0..max_iterations {
            // Uncorrected offenses will be found again, so only the corrected
            // ones from the previous iteration are kept.
            if let Some(last) = offenses_by_iteration.last_mut() {
                last.retain(Offense::corrected);
            }
            let new_source = {
                let buffer_source = crate::processed_source::normalize_newlines(&source);
                let processed_source =
                    ProcessedSource::new(&source, &buffer_source, ruby_version, file);
                let report = team.investigate(&processed_source);
                for error in &report.errors {
                    let _ = writeln!(crate::cli::stderr(), "{error}");
                }
                offenses_by_iteration.push(report.offenses);
                match report.corrector {
                    Some(corrector) if self.cop_options.autocorrect != AutocorrectMode::Off => {
                        corrector.rewrite(&buffer_source)
                    }
                    _ => break,
                }
            };
            if new_source == source || seen_sources.contains(&new_source) {
                break;
            }
            seen_sources.push(std::mem::replace(&mut source, new_source));
            corrected = true;
        }

        (corrected.then_some(source), offenses_by_iteration)
    }
}

fn source_for_stdin(stdin_source: Option<&str>) -> String {
    stdin_source.unwrap_or_default().to_owned()
}
