//! Port of `lib/rubocop/formatter/colorizable.rb` (Rainbow's ANSI colors).

use crate::cop::Severity;

#[derive(Debug, Clone, Copy)]
pub struct Colorizable {
    pub enabled: bool,
}

impl Colorizable {
    pub fn colorize(&self, text: &str, code: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    pub fn red(&self, text: &str) -> String {
        self.colorize(text, "31")
    }

    pub fn green(&self, text: &str) -> String {
        self.colorize(text, "32")
    }

    pub fn yellow(&self, text: &str) -> String {
        self.colorize(text, "33")
    }

    pub fn magenta(&self, text: &str) -> String {
        self.colorize(text, "35")
    }

    pub fn cyan(&self, text: &str) -> String {
        self.colorize(text, "36")
    }

    pub fn gray(&self, text: &str) -> String {
        self.colorize(text, "90")
    }

    pub fn severity_code(&self, severity: Severity) -> String {
        let code = severity.code().to_string();
        match severity {
            Severity::Info => self.gray(&code),
            Severity::Refactor | Severity::Convention => self.yellow(&code),
            Severity::Warning => self.magenta(&code),
            Severity::Error | Severity::Fatal => self.red(&code),
        }
    }
}
