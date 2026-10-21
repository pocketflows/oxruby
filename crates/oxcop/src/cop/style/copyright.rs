//! Port of `lib/rubocop/cop/style/copyright.rb`.

use crate::cop::mixin::range_help::{range_between, source_range};
use crate::cop::mixin::ruby_regex;
use crate::cop::{Cop, Investigation};
use crate::processed_source::ProcessedSource;
use crate::regex;

const AUTOCORRECT_EMPTY_WARNING: &str = "An AutocorrectNotice must be defined in your RuboCop config";

#[derive(Default)]
pub struct Copyright;

impl Cop for Copyright {
    fn name(&self) -> &'static str {
        "Style/Copyright"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let notice = cx.cop_config_str("Notice").unwrap_or("");
        if notice.is_empty() {
            return;
        }
        let Some(notice_regexp) = ruby_regex(
            &regex!(r"\A(?:\\A|\^)?#(?:\\s[*+?]?|\s)*").replace(notice, ""),
        ) else {
            return;
        };
        let ps = cx.processed_source;
        if notice_found(ps, &notice_regexp) {
            return;
        }
        let message = format!("Include a copyright notice matching /{notice}/ before any code.");
        if ps.is_blank() {
            cx.add_global_offense(&message);
            return;
        }
        let offense_range = source_range(ps, 1, 0, 1);
        if cx.autocorrect() {
            let autocorrect_notice = cx.cop_config_str("AutocorrectNotice").unwrap_or("");
            if let Err(warning) = verify_autocorrect_notice(autocorrect_notice, notice, &notice_regexp)
            {
                // `raise Warning` from the correction block: the offense is
                // not registered.
                crate::cop::base::raise(
                    "RuboCop::Warning",
                    format!("{}: {warning}", cx.cop_name()),
                );
            }
            let normalized = normalized_autocorrect_notice(autocorrect_notice);
            let insert_at = insert_notice_before(ps).unwrap_or(range_between(0, 0));
            cx.add_offense_with_correction(offense_range, &message, |corrector| {
                corrector.insert_before(insert_at, format!("{normalized}\n"));
            });
        } else {
            cx.add_offense_with_correction(offense_range, &message, |_| {});
        }
    }
}

fn normalized_autocorrect_notice(autocorrect_notice: &str) -> String {
    autocorrect_notice
        .split_inclusive('\n')
        .map(|line| {
            if line.starts_with('#') {
                line.to_owned()
            } else if line.trim_end_matches('\n').is_empty() {
                "#\n".to_owned()
            } else {
                format!("# {line}")
            }
        })
        .collect()
}

fn verify_autocorrect_notice(
    autocorrect_notice: &str,
    notice: &str,
    notice_regexp: &onig::Regex,
) -> Result<(), String> {
    if autocorrect_notice.is_empty() {
        return Err(AUTOCORRECT_EMPTY_WARNING.to_owned());
    }
    let stripped = regex!(r"^# *").replace_all(&normalized_autocorrect_notice(autocorrect_notice), "");
    if notice_regexp.find(&stripped).is_some() {
        return Ok(());
    }
    Err(format!(
        "AutocorrectNotice '{autocorrect_notice}' must match Notice /{notice}/"
    ))
}

/// The range of the token the notice goes before (after a shebang and an
/// encoding comment).
fn insert_notice_before(ps: &ProcessedSource<'_>) -> Option<crate::processed_source::Range> {
    let tokens = ps.tokens();
    let mut index = 0;
    let comment_matching = |index: usize, re: &onig::Regex| {
        tokens
            .get(index)
            .is_some_and(|t| t.ty == "tCOMMENT" && re.find(&t.text).is_some())
    };
    if comment_matching(index, regex!(r"\A#!.*\z")) {
        index += 1;
    }
    if comment_matching(index, regex!(r"\A#.*coding\s?[:=]\s?(?:UTF|utf)-8")) {
        index += 1;
    }
    tokens.get(index).map(|t| t.range)
}

fn notice_found(ps: &ProcessedSource<'_>, notice_regexp: &onig::Regex) -> bool {
    let mut multiline_notice = String::new();
    for token in ps.tokens() {
        if token.ty != "tCOMMENT" {
            break;
        }
        multiline_notice.push_str(&regex!(r"\A# *").replace(&token.text, ""));
        multiline_notice.push('\n');
        if notice_regexp.find(&token.text).is_some() {
            break;
        }
    }
    notice_regexp.find(&multiline_notice).is_some()
}
