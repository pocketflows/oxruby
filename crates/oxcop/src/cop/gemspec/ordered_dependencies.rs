//! Port of `lib/rubocop/cop/gemspec/ordered_dependencies.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::cop::mixin::ordered_gem_node::{
    case_insensitive_out_of_order, consecutive_lines, gem_name, register_offense,
};
use crate::cop::{Cop, Investigation};

static DEPENDENCY_DECLARATIONS: NodePattern = NodePattern::new(
    "(send (lvar _) {:add_dependency :add_runtime_dependency :add_development_dependency} (str _) ...)",
);

#[derive(Default)]
pub struct OrderedDependencies;

impl Cop for OrderedDependencies {
    fn name(&self) -> &'static str {
        "Gemspec/OrderedDependencies"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn on_new_investigation(&mut self, cx: &mut Investigation<'_, '_>) {
        let Some(ast) = cx.processed_source.ast() else {
            return;
        };
        let declarations: Vec<_> = DEPENDENCY_DECLARATIONS
            .search(ast)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
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
            if previous.method_name() != current.method_name() {
                continue;
            }
            register_offense(cx, previous, current, |previous, current| {
                format!(
                    "Dependencies should be sorted in an alphabetical order within their \
                     section of the gemspec. Dependency `{previous}` should appear before \
                     `{current}`."
                )
            });
        }
    }
}
