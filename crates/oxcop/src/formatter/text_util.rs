//! Port of `lib/rubocop/formatter/text_util.rb`.

pub fn pluralize(number: usize, thing: &str, no_for_zero: bool) -> String {
    if number == 0 && no_for_zero {
        format!("no {thing}s")
    } else if number == 1 {
        format!("1 {thing}")
    } else {
        format!("{number} {thing}s")
    }
}
