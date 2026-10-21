//! Port of `lib/rubocop/magic_comment.rb`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::regex;

const TOKEN: &str = r"(?<token>[[:alnum:]\-_]+)";

fn keyword(name: &str) -> &'static str {
    match name {
        "encoding" => "(?:en)?coding",
        "frozen_string_literal" => "frozen[_-]string[_-]literal",
        "rbs_inline" => "rbs_inline",
        "warn_indent" => "warn[_-]indent",
        "shareable_constant_value" => "shareable[_-]constant[_-]value",
        "typed" => "typed",
        _ => unreachable!(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Emacs,
    Vim,
    Simple,
}

/// A parsed magic comment line.
#[derive(Debug, Clone)]
pub struct MagicComment<'a> {
    comment: &'a str,
    kind: Kind,
}

/// The value of a `frozen_string_literal` comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrozenValue {
    True,
    False,
    Other(String),
}

fn ci_regex(pattern: &str) -> Rc<onig::Regex> {
    cached_regex(pattern, onig::RegexOptions::REGEX_OPTION_IGNORECASE)
}

thread_local! {
    static REGEX_CACHE: RefCell<HashMap<(String, bool), Rc<onig::Regex>>> =
        RefCell::new(HashMap::new());
}

/// Compiling an Oniguruma regex is slow, and cops ask about magic comments
/// once per node (e.g. Style/RedundantFreeze), so the few patterns built
/// here are compiled once per thread.
fn cached_regex(pattern: &str, options: onig::RegexOptions) -> Rc<onig::Regex> {
    let key = (
        pattern.to_owned(),
        options == onig::RegexOptions::REGEX_OPTION_IGNORECASE,
    );
    REGEX_CACHE.with(|cache| {
        Rc::clone(cache.borrow_mut().entry(key).or_insert_with(|| {
            Rc::new(onig::Regex::with_options(pattern, options, onig::Syntax::ruby()).unwrap())
        }))
    })
}

impl<'a> MagicComment<'a> {
    pub fn parse(comment: &'a str) -> Self {
        let kind = if regex!(r"-\*-(?<token>.+)-\*-").find(comment).is_some() {
            Kind::Emacs
        } else if regex!(r"#\s*vim:\s*(?<token>.+)").find(comment).is_some() {
            Kind::Vim
        } else {
            Kind::Simple
        };
        Self { comment, kind }
    }

    fn editor_tokens(&self) -> Vec<String> {
        let (re, separator) = match self.kind {
            Kind::Emacs => (regex!(r"-\*-(?<token>.+)-\*-"), ";"),
            Kind::Vim => (regex!(r"#\s*vim:\s*(?<token>.+)"), ", "),
            Kind::Simple => return Vec::new(),
        };
        let Some(caps) = re.captures(self.comment) else {
            return Vec::new();
        };
        caps.at(1)
            .unwrap_or("")
            .split(separator)
            .map(|t| t.trim().to_owned())
            .collect()
    }

    fn editor_match(&self, keyword: &str) -> Option<String> {
        let operator = if self.kind == Kind::Emacs { ":" } else { "=" };
        let pattern = regex_from(&format!(r"\A{keyword}\s*{operator}\s*{TOKEN}\z"));
        for token in self.editor_tokens() {
            if let Some(caps) = pattern.captures(&token)
                && let Some(value) = caps.at(1)
            {
                return Some(value.to_lowercase());
            }
        }
        None
    }

    fn simple_extract(&self, name: &str) -> Option<String> {
        let re = ci_regex(&format!(r"\A\s*#\s*{}:\s*{TOKEN}\s*\z", keyword(name)));
        re.captures(self.comment)
            .and_then(|c| c.at(1).map(str::to_owned))
    }

    fn extract_frozen_string_literal(&self) -> Option<String> {
        match self.kind {
            Kind::Emacs => self.editor_match(keyword("frozen_string_literal")),
            Kind::Vim => None,
            Kind::Simple => self.simple_extract("frozen_string_literal"),
        }
    }

    /// `frozen_string_literal`.
    pub fn frozen_string_literal(&self) -> Option<FrozenValue> {
        let setting = self.extract_frozen_string_literal()?;
        Some(match setting.to_lowercase().as_str() {
            "true" => FrozenValue::True,
            "false" => FrozenValue::False,
            _ => FrozenValue::Other(setting),
        })
    }

    pub fn is_frozen_string_literal(&self) -> bool {
        self.frozen_string_literal() == Some(FrozenValue::True)
    }

    /// `valid_literal_value?`
    pub fn valid_literal_value(&self) -> bool {
        matches!(
            self.frozen_string_literal(),
            Some(FrozenValue::True | FrozenValue::False)
        )
    }

    pub fn frozen_string_literal_specified(&self) -> bool {
        self.frozen_string_literal().is_some()
    }

    pub fn shareable_constant_value(&self) -> Option<String> {
        match self.kind {
            Kind::Emacs => self.editor_match(keyword("shareable_constant_value")),
            Kind::Vim => None,
            Kind::Simple => self.simple_extract("shareable_constant_value"),
        }
    }

    pub fn valid_shareable_constant_value(&self) -> bool {
        matches!(
            self.shareable_constant_value().as_deref(),
            Some("none" | "literal" | "experimental_everything" | "experimental_copy")
        )
    }

    pub fn warn_indent(&self) -> Option<String> {
        match self.kind {
            Kind::Emacs => self.editor_match(keyword("warn_indent")),
            Kind::Vim => None,
            Kind::Simple => self.simple_extract("warn_indent"),
        }
    }

    pub fn encoding(&self) -> Option<String> {
        match self.kind {
            Kind::Emacs => self.editor_match(keyword("encoding")),
            Kind::Vim => {
                if self.editor_tokens().len() > 1 {
                    self.editor_match("fileencoding")
                } else {
                    None
                }
            }
            Kind::Simple => {
                let re = ci_regex(&format!(
                    r"\A\s*\#\s*(frozen_string_literal:\s*(true|false))?\s*{}: ({TOKEN})",
                    keyword("encoding")
                ));
                re.captures(self.comment)
                    .and_then(|c| c.at(1).map(str::to_owned))
            }
        }
    }

    fn rbs_inline_value(&self) -> Option<String> {
        match self.kind {
            Kind::Simple => self.simple_extract("rbs_inline"),
            _ => None,
        }
    }

    pub fn typed(&self) -> Option<String> {
        match self.kind {
            Kind::Simple => self.simple_extract("typed"),
            _ => None,
        }
    }

    /// `any?`
    pub fn any(&self) -> bool {
        self.frozen_string_literal_specified()
            || self.encoding().is_some()
            || matches!(
                self.rbs_inline_value().as_deref(),
                Some("enabled" | "disabled")
            )
            || self.warn_indent().is_some()
            || self.shareable_constant_value().is_some()
            || self.typed().is_some()
    }

    /// `valid?`
    pub fn valid(&self) -> bool {
        self.comment.starts_with('#') && self.any()
    }
}

fn regex_from(pattern: &str) -> Rc<onig::Regex> {
    cached_regex(pattern, onig::RegexOptions::REGEX_OPTION_NONE)
}
