//! Port of `lib/rubocop/cop/mixin/require_library.rb`. A cop keeps a
//! [`RequireLibrary`] field, resets it in `on_new_investigation`, forwards
//! its `require` sends to [`RequireLibrary::on_send`] (restrict them with
//! [`RESTRICT_ON_SEND`]) and calls [`RequireLibrary::ensure_required`] from
//! its corrections.

use std::collections::HashSet;

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::Corrector;
use crate::cop::correctors::require_library_corrector;
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::processed_source::ProcessedSource;

pub const RESTRICT_ON_SEND: &[&str] = &["require"];

static REQUIRE_ANY_LIBRARY: NodePattern =
    NodePattern::new("(send {(const {nil? cbase} :Kernel) nil?} :require (str $_))");

static REQUIRE_LIBRARY_NAME: NodePattern =
    NodePattern::new("(send {(const {nil? cbase} :Kernel) nil?} :require (str %1))");

#[derive(Debug, Default)]
pub struct RequireLibrary {
    required_libs: HashSet<String>,
}

impl RequireLibrary {
    /// `on_new_investigation`.
    pub fn reset(&mut self) {
        self.required_libs.clear();
    }

    /// `on_send(node)`: records top-level `require 'lib'` calls.
    pub fn on_send(&mut self, node: Node<'_>) {
        if node.parent().is_some_and(|p| p.parent().is_some()) {
            return;
        }
        if let Some(name) = REQUIRE_ANY_LIBRARY
            .match_node(node)
            .and_then(|c| c[0].str().map(str::to_owned))
        {
            self.required_libs.insert(name);
        }
    }

    pub fn is_required(&self, library_name: &str) -> bool {
        self.required_libs.contains(library_name)
    }

    /// `ensure_required(corrector, node, library_name)`.
    pub fn ensure_required(
        &self,
        ps: &ProcessedSource<'_>,
        corrector: &mut Corrector,
        node: Node<'_>,
        library_name: &str,
    ) {
        let mut node = node;
        while let Some(parent) = node.parent().filter(|p| p.parent().is_some()) {
            node = parent;
        }
        if node.parent().is_some_and(|p| p.is(NodeType::Begin)) {
            if self.required_libs.contains(library_name) {
                return;
            }
            remove_subsequent_requires(ps, corrector, node, library_name);
        }
        require_library_corrector::correct(corrector, node, library_name);
    }
}

fn remove_subsequent_requires(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    node: Node<'_>,
    library_name: &str,
) {
    let params = [Param::Str(library_name)];
    let ctx = MatchContext {
        params: &params,
        ..MatchContext::default()
    };
    for sibling in node.right_siblings() {
        if REQUIRE_LIBRARY_NAME.match_with(sibling, &ctx).is_none() {
            continue;
        }
        corrector.remove(range_by_whole_lines(ps, sibling.source_range(), true));
    }
}
