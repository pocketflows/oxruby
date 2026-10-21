//! Port of `lib/rubocop/cop/mixin/unused_argument.rb`: common
//! functionality for cops that check unused arguments. The cop implements
//! `after_leaving_scope` by calling [`check_argument`] for each variable,
//! after its own checks (Ruby's `super`).

use crate::cop::Investigation;
use crate::cop::correctors::unused_arg_corrector;
use crate::cop::variable_force::Variable;

/// `check_argument(variable)`, with the cop's `message`.
pub fn check_argument<'a>(
    cx: &mut Investigation<'a, '_>,
    variable: &Variable<'a>,
    message: impl FnOnce(&Variable<'a>) -> String,
) {
    if variable.should_be_unused() {
        return;
    }
    if variable.is_referenced() {
        return;
    }

    let message = message(variable);

    let node = variable.declaration_node;
    let Some(range) = node.loc().name else {
        return;
    };
    let processed_source = cx.processed_source;
    cx.add_offense_with_correction(range, &message, |corrector| {
        unused_arg_corrector::correct(corrector, processed_source, node);
    });
}
