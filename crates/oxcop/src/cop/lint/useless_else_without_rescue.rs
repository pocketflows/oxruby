//! Port of `lib/rubocop/cop/lint/useless_else_without_rescue.rb`.
//!
//! The cop reports the `parser` gem's `:useless_else` diagnostic, which only
//! exists for Ruby 2.5 and older (`maximum_target_ruby_version 2.5`). Prism
//! reports `else` without `rescue` as a syntax error instead, so this cop
//! never registers an offense (the upstream examples are all
//! `unsupported_on: :prism`).

use crate::cop::{Cop, Investigation};

#[derive(Default)]
pub struct UselessElseWithoutRescue;

impl Cop for UselessElseWithoutRescue {
    fn name(&self) -> &'static str {
        "Lint/UselessElseWithoutRescue"
    }

    fn maximum_target_ruby_version(&self) -> Option<f64> {
        Some(2.5)
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        // `processed_source.diagnostics` never has a `:useless_else` reason.
    }
}
