//! Port of `lib/rubocop/runner.rb`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_yaml_ng::{Mapping, Value};

use crate::config::Config;
use crate::config_loader::{ConfigError, ConfigStore};
use crate::cop::base::{AutoGenState, CopError, CopOptions, OffenseToCheck};
use crate::cop::team::{Selection, Team};
use crate::cop::{Offense, Severity};
use crate::formatter::Formatter;
use crate::options::{AutocorrectMode, FailLevel, Options};
use crate::path_util;
use crate::processed_source::ProcessedSource;
use crate::source_encoding::{self, SourceEncoding};
use crate::target_finder::TargetFinder;
use crate::target_ruby::target_ruby_version;

pub const MAX_ITERATIONS: usize = 200;

/// `PendingCopsReporter::PENDING_BANNER`.
const PENDING_BANNER: &str = "The following cops were added to RuboCop, but are not configured. \
Please set Enabled to either `true` or `false` in your `.rubocop.yml` file.

Please also note that you can opt-in to new cops by default by adding this to your config:
  AllCops:
    NewCops: enable
";

pub struct Runner<'a> {
    options: &'a Options,
    config_store: ConfigStore,
    inspector: Inspector<'a>,
    /// With `--stdin` and autocorrect, the corrected source to echo back.
    pub corrected_stdin: Option<Vec<u8>>,
    pub errors: Vec<String>,
    /// `--spec-report`: what the spec shim needs besides the offenses.
    pub spec_report: SpecReport,
}

/// Cop state RuboCop's specs inspect on the cop object or in class-level
/// hashes, which the shim copies back into Ruby (see `spec/oxcop/`).
#[derive(Debug, Default)]
pub struct SpecReport {
    /// Per cop, the state after the last investigation that had any.
    pub auto_gen: std::collections::BTreeMap<String, AutoGenState>,
    /// Errors raised by cops, in order.
    pub raised: Vec<CopError>,
    /// What `cli::warn` printed.
    pub warnings: Vec<String>,
}

impl SpecReport {
    pub fn to_json(&self) -> serde_json::Value {
        let yaml_to_json =
            |value: &Mapping| serde_json::to_value(value).unwrap_or(serde_json::Value::Null);
        let mut config_to_allow_offenses = serde_json::Map::new();
        let mut detected_styles = serde_json::Map::new();
        let mut exclude_limits = serde_json::Map::new();
        for (cop, state) in &self.auto_gen {
            config_to_allow_offenses
                .insert(cop.clone(), yaml_to_json(&state.config_to_allow_offenses));
            detected_styles.insert(cop.clone(), serde_json::json!(state.detected_style));
            let limits: Vec<serde_json::Value> = state
                .exclude_limits
                .iter()
                .map(|(param, value)| serde_json::json!([param, value]))
                .collect();
            exclude_limits.insert(cop.clone(), limits.into());
        }
        let raised: Vec<serde_json::Value> = self
            .raised
            .iter()
            .map(|e| serde_json::json!({ "class": e.class, "message": e.message }))
            .collect();
        serde_json::json!({
            "config_to_allow_offenses": config_to_allow_offenses,
            "detected_styles": detected_styles,
            "exclude_limits": exclude_limits,
            "raised": raised,
            "warnings": self.warnings,
        })
    }
}

/// The per-file work, which needs nothing mutable from the runner, so files
/// can be inspected on rayon's threads (`--parallel`, the default).
struct Inspector<'a> {
    options: &'a Options,
    selection: Selection,
    cop_options: CopOptions,
    /// `--auto-gen-state`: the state RuboCop's class-level hashes hold when
    /// the run starts (the spec shim's current values).
    auto_gen_seed: std::collections::BTreeMap<String, AutoGenState>,
    /// `--offenses-to-check`.
    offenses_to_check: Option<Vec<OffenseToCheck>>,
}

/// What inspecting one file produced, reported in file order.
#[derive(Default)]
struct FileResult {
    offenses: Vec<Offense>,
    /// Errors raised by cops, printed when the file is reported.
    cop_errors: Vec<String>,
    /// Errors reading or writing the file (`Runner#errors`).
    errors: Vec<String>,
    corrected_stdin: Option<Vec<u8>>,
    raised: Vec<CopError>,
    auto_gen: Vec<(String, AutoGenState)>,
    /// A `RuboCop::ValidationError`, which stops the run.
    validation_error: Option<CopError>,
}

impl<'a> Runner<'a> {
    pub fn new(options: &'a Options, cop_options: CopOptions) -> Self {
        let mut config_store = ConfigStore::new(options.config.clone());
        config_store.raw = options.raw_config;
        // `Registry#enabled_pending_cop?` checks `--disable-pending-cops` first.
        config_store.pending_cops_option = if options.disable_pending_cops {
            Some(false)
        } else if options.enable_pending_cops {
            Some(true)
        } else {
            None
        };
        Self {
            options,
            config_store,
            inspector: Inspector {
                options,
                selection: Selection {
                    only: only_cops(options),
                    except: options.except.clone(),
                    registry_from_config: options.raw_config,
                },
                cop_options,
                auto_gen_seed: options
                    .auto_gen_state
                    .as_deref()
                    .map(read_auto_gen_state)
                    .unwrap_or_default(),
                offenses_to_check: options
                    .offenses_to_check
                    .as_deref()
                    .map(read_offenses_to_check),
            },
            corrected_stdin: None,
            errors: Vec::new(),
            spec_report: SpecReport::default(),
        }
    }

    /// `PendingCopsReporter.warn_if_needed(config_store.for_pwd)`.
    pub fn pending_cops_warning(&mut self) -> Result<Option<String>, ConfigError> {
        if self.options.disable_pending_cops || self.options.enable_pending_cops {
            return Ok(None);
        }
        let config = self.config_store.for_pwd()?;
        let known = config.known_cops();
        let pending: Vec<(&str, &Mapping)> = config
            .pending_cops()
            .into_iter()
            .filter(|(name, _)| known.contains(name))
            .collect();
        if pending.is_empty() {
            return Ok(None);
        }
        let mut out = String::from(PENDING_BANNER);
        for (name, params) in pending {
            let version = params
                .get("VersionAdded")
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => "N/A".to_owned(),
                })
                .unwrap_or_else(|| "N/A".to_owned());
            out.push_str(&format!("{name}: # new in {version}\n  Enabled: true\n"));
        }
        out.push_str("For more information: https://docs.rubocop.org/rubocop/versioning.html\n");
        Ok(Some(out))
    }

    pub fn find_target_files(&mut self) -> Result<Vec<PathBuf>, ConfigError> {
        if let Some(stdin_path) = &self.options.stdin {
            if self.options.keep_stdin_path {
                return Ok(vec![stdin_path.clone()]);
            }
            return Ok(vec![path_util::expand_path(stdin_path)]);
        }
        let mut files = TargetFinder::new(&mut self.config_store, self.options.force_exclusion)
            .find(&self.options.paths)?;
        if self.options.fail_fast {
            // `TargetFinder#order`: most recently modified file first.
            files.sort_by_cached_key(|file| {
                let mtime = std::fs::metadata(file)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                std::cmp::Reverse(mtime)
            });
        }
        Ok(files)
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
                let mut source = Vec::new();
                stdin
                    .read_to_end(&mut source)
                    .map_err(|e| ConfigError(format!("reading stdin: {e}")))?;
                Some(source)
            }
            None => None,
        };

        formatter.started(&files);
        let mut all_passed = true;
        let mut inspected = Vec::new();
        let mut report =
            |runner: &mut Self, formatter: &mut dyn Formatter, file: &Path, result: FileResult| {
                for error in &result.cop_errors {
                    let _ = writeln!(crate::cli::stderr(), "{error}");
                }
                if let Some(error) = result.validation_error {
                    let message = error.message.clone();
                    runner.spec_report.raised.push(error);
                    return Err(ConfigError(message));
                }
                runner.errors.extend(result.errors);
                runner.spec_report.raised.extend(result.raised);
                for (cop, state) in result.auto_gen {
                    runner.spec_report.auto_gen.insert(cop, state);
                }
                if result.corrected_stdin.is_some() {
                    runner.corrected_stdin = result.corrected_stdin;
                }
                let minimum = runner.minimum_severity_to_fail()?;
                let options = runner.options;
                let succeeded = !result.offenses.iter().any(|offense| {
                    considered_failure(options, minimum, offense)
                        && offense_displayed(options, minimum, offense)
                });
                all_passed &= succeeded;
                // `offenses_to_report`
                let offenses: Vec<Offense> = result
                    .offenses
                    .into_iter()
                    .filter(|offense| offense_displayed(options, minimum, offense))
                    .collect();
                formatter.file_finished(file, &offenses);
                inspected.push(file.to_path_buf());
                Ok::<bool, ConfigError>(succeeded)
            };

        if self.run_in_parallel(&files) {
            let configs = files
                .iter()
                .map(|file| self.config_store.for_file(file))
                .collect::<Result<Vec<_>, _>>()?;
            let configs: Vec<&Config> = configs.iter().map(|config| &**config).collect();
            let inspector = &self.inspector;
            let results: Vec<FileResult> = files
                .par_iter()
                .zip(configs.par_iter())
                .map(|(file, config)| inspector.file_offenses(file, None, config))
                .collect();
            for (file, result) in files.iter().zip(results) {
                formatter.file_started(file);
                report(self, formatter, file, result)?;
            }
        } else {
            for file in &files {
                formatter.file_started(file);
                let config = self.config_store.for_file(file)?;
                let result = self
                    .inspector
                    .file_offenses(file, stdin_source.as_deref(), &config);
                let succeeded = report(self, formatter, file, result)?;
                if self.options.fail_fast && !succeeded {
                    break;
                }
            }
        }
        formatter.finished(&inspected);
        Ok(all_passed)
    }

    /// `Runner#run_in_parallel?`. Parallel by default, as RuboCop is; the
    /// offenses and their order are the same either way.
    /// (`-F/--fail-fast` is incompatible with parallel inspection.)
    fn run_in_parallel(&self, files: &[PathBuf]) -> bool {
        !self.options.no_parallel
            && !self.options.fail_fast
            && self.options.stdin.is_none()
            && files.len() > 1
    }

    /// `Severity.minimum_to_fail(options, config_store.for_pwd)`.
    fn minimum_severity_to_fail(&mut self) -> Result<Severity, ConfigError> {
        if let Some(FailLevel::Severity(severity)) = self.options.fail_level {
            return Ok(severity);
        }
        let config = self.config_store.for_pwd()?;
        Ok(config
            .for_all_cops()
            .get("FailLevel")
            .and_then(Value::as_str)
            .and_then(Severity::from_code_or_name)
            .unwrap_or(Severity::Refactor))
    }
}

/// `Runner#considered_failure?`.
fn considered_failure(options: &Options, minimum: Severity, offense: &Offense) -> bool {
    if offense.disabled() {
        return false;
    }
    if options.fail_level == Some(FailLevel::Autocorrect) && offense.correctable() {
        return true;
    }
    !offense.corrected() && offense.severity >= minimum
}

/// `Runner#offense_displayed?`.
fn offense_displayed(options: &Options, minimum: Severity, offense: &Offense) -> bool {
    if options.display_only_fail_level_offenses {
        considered_failure(options, minimum, offense)
    } else if options.display_only_safe_correctable {
        supports_safe_autocorrect(offense)
    } else if options.display_only_correctable {
        offense.correctable()
    } else {
        true
    }
}

/// `Runner#supports_safe_autocorrect?`: correctable, and safe by the cop's
/// default configuration.
fn supports_safe_autocorrect(offense: &Offense) -> bool {
    let default = crate::config_loader::default_configuration()
        .get(offense.cop_name.as_str())
        .and_then(Value::as_mapping);
    let flag = |key: &str| {
        default
            .and_then(|d| d.get(key))
            .is_none_or(|v| v != &Value::Bool(false))
    };
    offense.correctable() && flag("Safe") && flag("SafeAutoCorrect")
}

impl Inspector<'_> {
    fn file_offenses(
        &self,
        file: &Path,
        stdin_source: Option<&[u8]>,
        config: &Config,
    ) -> FileResult {
        let mut result = FileResult::default();
        let bytes = match stdin_source {
            Some(source) => std::borrow::Cow::Borrowed(source),
            None => match std::fs::read(file) {
                Ok(bytes) => std::borrow::Cow::Owned(bytes),
                Err(e) => {
                    result.errors.push(format!("{}: {e}", file.display()));
                    return result;
                }
            },
        };
        // `ProcessedSource#parse` rescues the buffer's `EncodingError`: the
        // file is investigated as one without valid syntax (Lint/Syntax
        // reports the error), with no source to correct.
        let (source, encoding, parser_error) = match source_encoding::decode(&bytes) {
            Ok((source, encoding)) => (source, encoding, None),
            Err(error) => (String::new(), SourceEncoding::default(), Some(error)),
        };
        let (mut corrected_source, offenses) = self.do_inspection_loop(
            file,
            source.clone(),
            parser_error.as_deref(),
            config,
            &mut result,
        );
        let mut offenses: Vec<Offense> = offenses.into_iter().flatten().collect();
        offenses.sort();

        // add_redundant_disables (a source with a parser error has no
        // comments, so no directives).
        let current = corrected_source.as_deref().unwrap_or(&source);
        if parser_error.is_none()
            && let Some((new_offenses, updated)) =
                self.redundant_disables(file, current, config, &offenses, &mut result)
        {
            offenses.extend(new_offenses);
            if let Some(updated) = updated {
                // One more loop: removing a disable can make an enable
                // redundant.
                let (again, more) =
                    self.do_inspection_loop(file, updated.clone(), None, config, &mut result);
                corrected_source = Some(again.unwrap_or(updated));
                for offense in more.into_iter().flatten() {
                    if !offenses.contains(&offense) {
                        offenses.push(offense);
                    }
                }
            }
        }

        if let Some(corrected) = corrected_source {
            let corrected = source_encoding::encode(&corrected, encoding);
            if stdin_source.is_some() {
                result.corrected_stdin = Some(corrected);
            } else if let Err(e) = std::fs::write(file, corrected) {
                result.errors.push(format!("{}: {e}", file.display()));
            }
        } else if let Some(source) = stdin_source {
            result.corrected_stdin = Some(source.to_vec());
        }

        offenses.retain(|offense| !offense.disabled());
        offenses.sort();
        // The runner's `offenses_by_iteration.flatten.uniq` merges offenses
        // with the same line, column and message. RuboCop's specs look at the
        // cop's own offenses, before that, so the spec shim keeps them all.
        if !self.options.raw_config {
            offenses.dedup();
        }
        result.offenses = offenses;
        result
    }

    /// `Runner#add_redundant_disables`: Lint/RedundantCopDisableDirective
    /// checks the directives against every offense (disabled ones too). Its
    /// offenses, and the corrected source when it removed directives.
    fn redundant_disables(
        &self,
        file: &Path,
        source: &str,
        config: &Config,
        offenses: &[Offense],
        result: &mut FileResult,
    ) -> Option<(Vec<Offense>, Option<String>)> {
        const COP: &str = "Lint/RedundantCopDisableDirective";
        // except_redundant_cop_disable_directive? and `!@options[:only]`
        let except = &self.options.except;
        if [
            "Lint/RedundantCopDisableDirective",
            "RedundantCopDisableDirective",
            "Lint",
        ]
        .iter()
        .any(|name| except.iter().any(|e| e == name))
            || !self.options.only.is_empty()
        {
            return None;
        }
        let enabled = config.for_cop(COP).get("Enabled");
        if enabled.is_none_or(|e| matches!(e, Value::Bool(false) | Value::Null))
            || !config.cop_relevant_file(COP, file)
        {
            return None;
        }
        let mut cop = crate::cop::registry::all()
            .iter()
            .map(|new| new())
            .find(|cop| cop.name() == COP)?;
        cop.set_offenses_to_check(offenses.iter().map(OffenseToCheck::from).collect());

        let ruby_version = target_ruby_version(config);
        let buffer_source = crate::processed_source::normalize_newlines(source);
        let processed_source = ProcessedSource::new(source, &buffer_source, ruby_version, file);
        let team = Team::with_cops(config, &self.selection, self.cop_options, vec![cop]);
        let report = team.investigate(&processed_source);
        // check_for_redundant_disables?
        if report.comment_config.cop_disabled_line_ranges.is_empty()
            && report.comment_config.detached_next_directives.is_empty()
        {
            return None;
        }
        result.cop_errors.extend(report.errors);
        result.raised.extend(report.raised);
        let updated = match report.corrector {
            Some(corrector) if self.cop_options.autocorrect != AutocorrectMode::Off => {
                Some(corrector.rewrite(&buffer_source)).filter(|new| new != source)
            }
            _ => None,
        };
        Some((report.offenses, updated))
    }

    /// `--prior`: investigates earlier sources with the team's cops, which it
    /// then keeps for the real source (offenses are discarded).
    fn investigate_priors(&self, team: &mut Team<'_>, ruby_version: f64) {
        team.persist_cops = true;
        for prior in &self.options.prior {
            let Some((name, file)) = prior.rsplit_once('=') else {
                continue;
            };
            let Ok(bytes) = std::fs::read(file) else {
                continue;
            };
            let (source, _) = source_encoding::decode_lossy(&bytes);
            let buffer_source = crate::processed_source::normalize_newlines(&source);
            let path = Path::new(name);
            let processed_source =
                ProcessedSource::new(&source, &buffer_source, ruby_version, path);
            crate::cli::quietly(|| team.investigate(&processed_source));
        }
    }

    /// Inspects (and with autocorrect, re-inspects) until no more corrections
    /// are made. Returns the corrected source, if any, and the offenses found
    /// in each iteration.
    fn do_inspection_loop(
        &self,
        file: &Path,
        mut source: String,
        parser_error: Option<&str>,
        config: &Config,
        result: &mut FileResult,
    ) -> (Option<String>, Vec<Vec<Offense>>) {
        let ruby_version = target_ruby_version(config);
        let mut team = Team::new(config, &self.selection, self.cop_options);
        team.persist_cops = self.options.persist_cops;
        if !self.options.prior.is_empty() {
            self.investigate_priors(&mut team, ruby_version);
        }
        *team.auto_gen.borrow_mut() = self.auto_gen_seed.clone();
        team.offenses_to_check = self.offenses_to_check.clone();
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
                let processed_source = match parser_error {
                    Some(error) => {
                        ProcessedSource::with_parser_error(ruby_version, file, error.to_owned())
                    }
                    None => ProcessedSource::new(&source, &buffer_source, ruby_version, file),
                };
                // `--quiet-first-pass`: the spec shim's autocorrect run
                // repeats the investigation it just reported.
                let report = if self.options.quiet_first_pass && offenses_by_iteration.is_empty() {
                    crate::cli::quietly(|| team.investigate(&processed_source))
                } else {
                    team.investigate(&processed_source)
                };
                if report.validation_error.is_some() {
                    result.validation_error = report.validation_error;
                    break;
                }
                result.cop_errors.extend(report.errors);
                result.raised.extend(report.raised);
                // The team's state so far, which includes earlier iterations.
                result.auto_gen = report.auto_gen;
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

/// `--only`, with `Lint/Syntax` first, as `Options#add_cop_selection_csv_option`
/// adds it (not for RuboCop's specs, which run the cop alone).
fn only_cops(options: &Options) -> Vec<String> {
    let mut only = options.only.clone();
    if !options.raw_config && !only.is_empty() && !only.iter().any(|cop| cop == "Lint/Syntax") {
        only.insert(0, "Lint/Syntax".to_owned());
    }
    only
}

/// Reads `--auto-gen-state`: `{"config_to_allow_offenses": {cop: {...}},
/// "detected_styles": {cop: [...]}}`.
fn read_auto_gen_state(path: &Path) -> std::collections::BTreeMap<String, AutoGenState> {
    let mut states = std::collections::BTreeMap::<String, AutoGenState>::new();
    let Ok(json) = std::fs::read_to_string(path) else {
        return states;
    };
    let Ok(serde_json::Value::Object(json)) = serde_json::from_str(&json) else {
        return states;
    };
    if let Some(serde_json::Value::Object(configs)) = json.get("config_to_allow_offenses") {
        for (cop, config) in configs {
            if let Ok(Value::Mapping(config)) = serde_yaml_ng::to_value(config) {
                states
                    .entry(cop.clone())
                    .or_default()
                    .config_to_allow_offenses = config;
            }
        }
    }
    if let Some(serde_json::Value::Object(styles)) = json.get("detected_styles") {
        for (cop, detected) in styles {
            if let Some(list) = detected.as_array() {
                states.entry(cop.clone()).or_default().detected_style = Some(
                    list.iter()
                        .filter_map(|s| s.as_str().map(str::to_owned))
                        .collect(),
                );
            }
        }
    }
    states
}

/// Reads `--offenses-to-check`.
fn read_offenses_to_check(path: &Path) -> Vec<OffenseToCheck> {
    let json: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    json.as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            let line = o.get("line")?.as_u64()? as usize;
            Some(OffenseToCheck {
                cop_name: o.get("cop_name")?.as_str()?.to_owned(),
                line,
                last_line: o
                    .get("last_line")
                    .and_then(serde_json::Value::as_u64)
                    .map_or(line, |l| l as usize),
            })
        })
        .collect()
}
