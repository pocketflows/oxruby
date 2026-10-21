//! Port of `lib/rubocop/cop/style/disable_cops_within_source_code_directive.rb`.

use crate::cop::mixin::ruby_regex;
use crate::cop::mixin::util::regexp_escape;
use crate::cop::{Cop, Investigation};
use crate::directive_comment::DirectiveComment;
use crate::processed_source::Comment;
use crate::regex;

const MSG: &str = "RuboCop disable/enable directives are not permitted.";
const MSG_MISSING_REASON: &str =
    "RuboCop disable directives without a `--` justification comment are not permitted.";

#[derive(Default)]
pub struct DisableCopsWithinSourceCodeDirective;

impl Cop for DisableCopsWithinSourceCodeDirective {
    fn name(&self) -> &'static str {
        "Style/DisableCopsWithinSourceCodeDirective"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let allowed_directives = cx.cop_config_list("AllowedDirectives");
        let allow_with_reason = cx.cop_config_bool("AllowWithReason", false);
        let allowed_cops = cx.cop_config_list("AllowedCops");
        let disallowed_config = cx.cop_config_list("DisallowedCops");
        let comments: Vec<Comment<'_>> = cx.processed_source.comments().to_vec();
        for comment in comments {
            let directive = DirectiveComment::new(comment.text, comment.range, 0);
            let exempt = directive
                .mode
                .as_deref()
                .is_some_and(|m| allowed_directives.contains(&m));
            if exempt || (allow_with_reason && (directive.reason().is_some() || directive.enabled()))
            {
                continue;
            }
            let directive_cops: Vec<String> = directive
                .cops
                .as_deref()
                .map(|cops| cops.split(',').map(|c| c.trim().to_owned()).collect())
                .unwrap_or_default();
            let disallowed_cops: Vec<String> = if !disallowed_config.is_empty() {
                if directive_cops.iter().any(|c| c == "all") {
                    directive_cops.clone()
                } else {
                    let mut unique: Vec<String> = Vec::new();
                    for cop in &directive_cops {
                        if !unique.contains(cop) && listed(&disallowed_config, cop) {
                            unique.push(cop.clone());
                        }
                    }
                    unique
                }
            } else {
                directive_cops
                    .iter()
                    .filter(|cop| !(*cop != "all" && listed(&allowed_cops, cop)))
                    .cloned()
                    .collect()
            };
            if disallowed_cops.is_empty() {
                continue;
            }
            let message = if allow_with_reason {
                MSG_MISSING_REASON.to_owned()
            } else if !allowed_cops.is_empty() || !disallowed_config.is_empty() {
                format!(
                    "RuboCop disable/enable directives for `{}` are not permitted.",
                    disallowed_cops.join("`, `")
                )
            } else {
                MSG.to_owned()
            };
            cx.add_offense_with_correction(comment.range, &message, |corrector| {
                let mut replacement = String::new();
                if directive_cops.len() != disallowed_cops.len() {
                    let union: Vec<String> = disallowed_cops.iter().map(|c| regexp_escape(c)).collect();
                    if let Some(re) = ruby_regex(&format!(r"(?:{}),?\s*", union.join("|"))) {
                        let removed = match re.find(comment.text) {
                            Some((start, end)) => {
                                format!("{}{}", &comment.text[..start], &comment.text[end..])
                            }
                            None => comment.text.to_owned(),
                        };
                        replacement = match regex!(r",\s*$").find(&removed) {
                            Some((start, end)) => format!("{}{}", &removed[..start], &removed[end..]),
                            None => removed,
                        };
                    }
                }
                corrector.replace(comment.range, replacement);
            });
        }
    }
}

fn listed(names: &[&str], cop: &str) -> bool {
    names.contains(&cop) || names.contains(&cop.split('/').next().unwrap_or(""))
}

