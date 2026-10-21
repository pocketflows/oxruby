//! Port of `lib/rubocop/cop/bundler/ordered_gems.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::cop::mixin::ordered_gem_node::{
    case_insensitive_out_of_order, consecutive_lines, gem_name, register_offense,
};
use crate::cop::{Cop, Investigation};

static GEM_DECLARATIONS: NodePattern = NodePattern::new("(:send nil? :gem str ...)");

#[derive(Default)]
pub struct OrderedGems;

impl Cop for OrderedGems {
    fn name(&self) -> &'static str {
        "Bundler/OrderedGems"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let Some(ast) = cx.processed_source.ast() else {
            return;
        };
        let declarations: Vec<_> = GEM_DECLARATIONS.search(ast).into_iter().map(|(n, _)| n).collect();
        for pair in declarations.windows(2) {
            let (previous, current) = (pair[0], pair[1]);
            if !consecutive_lines(cx, previous, current) {
                continue;
            }
            if !case_insensitive_out_of_order(
                cx,
                gem_name(current).unwrap_or(""),
                gem_name(previous).unwrap_or(""),
            ) {
                continue;
            }
            register_offense(cx, previous, current, |previous, current| {
                format!(
                    "Gems should be sorted in an alphabetical order within their section of \
                     the Gemfile. Gem `{previous}` should appear before `{current}`."
                )
            });
        }
    }
}
