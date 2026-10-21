//! Port of `lib/rubocop/cop/gemspec/require_mfa.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::mixin::gemspec_help::gem_specification_p;
use crate::cop::{Cop, Corrector, Investigation};

const MSG: &str = "`metadata['rubygems_mfa_required']` must be set to `'true'`.";

static METADATA: NodePattern = NodePattern::new(
    r#"`{
       (send _ :metadata= $_)
       (send (send _ :metadata) :[]= {(str "rubygems_mfa_required") (sym :rubygems_mfa_required)} $_)
     }"#,
);

static METADATA_ASSIGNMENT: NodePattern = NodePattern::new(
    "`{
       (send _ :metadata= _)
       (send (send _ :metadata) :[]= {str sym} _)
     }",
);

static RUBYGEMS_MFA_REQUIRED: NodePattern = NodePattern::new(
    r#"(pair {(str "rubygems_mfa_required") (sym :rubygems_mfa_required)} $_)"#,
);

static TRUE_STRING: NodePattern = NodePattern::new(r#"(str "true")"#);

#[derive(Default)]
pub struct RequireMfa;

impl Cop for RequireMfa {
    fn name(&self) -> &'static str {
        "Gemspec/RequireMFA"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Block]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        // `gem_specification(node) do |block_var|`: a search from `node`.
        let block_vars: Vec<&str> = node.each_node().filter_map(gem_specification_p).collect();
        for block_var in block_vars {
            let metadata_value = METADATA.match_node(node).and_then(|c| c[0].node());
            match mfa_value(metadata_value) {
                Some(mfa_value) => {
                    if !TRUE_STRING.is_match(mfa_value) {
                        cx.add_offense_with_correction(mfa_value.source_range(), MSG, |corrector| {
                            corrector.replace(mfa_value.source_range(), "'true'");
                        });
                    }
                }
                None => {
                    cx.add_offense_with_correction(node.source_range(), MSG, |corrector| {
                        autocorrect(corrector, node, block_var, metadata_value);
                    });
                }
            }
        }
    }
}

fn mfa_value(metadata_value: Option<Node<'_>>) -> Option<Node<'_>> {
    let metadata_value = metadata_value?;
    if metadata_value.is(NodeType::Str) {
        return Some(metadata_value);
    }
    RUBYGEMS_MFA_REQUIRED
        .search(metadata_value)
        .into_iter()
        .next()
        .and_then(|(_, c)| c[0].node())
}

fn autocorrect(corrector: &mut Corrector, node: Node<'_>, block_var: &str, metadata: Option<Node<'_>>) {
    match metadata {
        Some(metadata) => {
            if metadata.is(NodeType::Hash) {
                correct_metadata(corrector, metadata);
            }
        }
        None => insert_mfa_required(corrector, node, block_var),
    }
}

fn correct_metadata(corrector: &mut Corrector, metadata: Node<'_>) {
    match metadata.pairs().last() {
        Some(last) => corrector.insert_after(
            last.source_range(),
            ",\n'rubygems_mfa_required' => 'true'",
        ),
        None => corrector.insert_before(
            metadata.loc().end.unwrap(),
            "'rubygems_mfa_required' => 'true'",
        ),
    }
}

fn insert_mfa_required(corrector: &mut Corrector, node: Node<'_>, block_var: &str) {
    let require_mfa_directive = format!("{block_var}.metadata['rubygems_mfa_required'] = 'true'");
    // Scope the search to the current spec block. Searching the whole file
    // would, for a second `Gem::Specification.new` block, insert the
    // directive into the first block, leaving this block uncorrected and
    // looping forever.
    match METADATA_ASSIGNMENT.search(node).into_iter().last() {
        Some((last_assignment, _)) => corrector.insert_after(
            last_assignment.source_range(),
            format!("\n{require_mfa_directive}"),
        ),
        None => corrector.insert_before(
            node.loc().end.unwrap(),
            format!("{require_mfa_directive}\n"),
        ),
    }
}
