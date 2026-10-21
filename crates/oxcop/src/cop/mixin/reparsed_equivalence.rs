//! Port of `lib/rubocop/cop/mixin/reparsed_equivalence.rb`: checks that a
//! correction still parses, or parses to the same AST, before an offense is
//! reported as correctable.
//!
//! A cop implements [`ReparsedEquivalence`] for the items it corrects (nodes
//! or ranges) and calls [`correction_parses`] or [`verified_by_reparse`].
//! Ruby's `normalize_reparsed_ast` hook becomes [`ReparsedEquivalence::reparsed_equal`],
//! which compares the original and the reparsed tree (structural equality by
//! default).

use std::borrow::Cow;

use crate::ast::{Ast, Node, NodeType};
use crate::cop::{Corrector, Investigation};
use crate::processed_source::Range;

pub const MAX_VERIFICATION_FRAGMENT_SIZE: usize = 64 * 1024;

/// `oversized:` of `verified_by_reparse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Oversized {
    /// Report items in a scope too large to verify without checking them.
    Report,
    /// Verify them anyway.
    Verify,
}

pub trait ReparsedEquivalence<I: Copy> {
    /// `apply_reparse_correction(corrector, item)`.
    fn apply_reparse_correction(
        &self,
        cx: &Investigation<'_, '_>,
        corrector: &mut Corrector,
        item: I,
    );

    /// `item_range(item)`: the item's source range.
    fn item_range(&self, item: I) -> Range;

    /// `preprocess_reparsed_source(source)`.
    fn preprocess_reparsed_source<'s>(&self, source: &'s str) -> Cow<'s, str> {
        Cow::Borrowed(source)
    }

    /// `normalize_reparsed_ast(original) == normalize_reparsed_ast(rewritten)`.
    fn reparsed_equal(&self, original: Node<'_>, rewritten: Node<'_>) -> bool {
        original.same_as(rewritten)
    }
}

/// `verified_by_reparse(items, oversized:)`: the items whose corrections
/// parse to an equivalent AST, checked together per enclosing scope and one
/// by one when the group fails.
pub fn verified_by_reparse<I: Copy, C: ReparsedEquivalence<I> + ?Sized>(
    cop: &C,
    cx: &Investigation<'_, '_>,
    items: &[I],
    oversized: Oversized,
) -> Vec<I> {
    let mut result = Vec::new();
    for (scope, group) in scope_groups(cop, cx, items) {
        if (oversized == Oversized::Report && verification_too_large(cx, scope))
            || (group.len() > 1 && corrections_verify(cop, cx, scope, &group))
        {
            result.extend(group);
        } else {
            result.extend(
                group
                    .into_iter()
                    .filter(|item| corrections_verify(cop, cx, scope, &[*item])),
            );
        }
    }
    result
}

/// `correction_parses?(item)`: the source with the item's correction is
/// still valid syntax.
pub fn correction_parses<I: Copy, C: ReparsedEquivalence<I> + ?Sized>(
    cop: &C,
    cx: &Investigation<'_, '_>,
    item: I,
) -> bool {
    let source = cx.processed_source.buffer_source();
    let mut corrector = Corrector::new(source.len());
    cop.apply_reparse_correction(cx, &mut corrector, item);
    if corrector.is_clobbered() {
        return false;
    }
    let corrected = corrector.rewrite(source);
    valid_syntax(cx, &corrected)
}

fn valid_syntax(cx: &Investigation<'_, '_>, source: &str) -> bool {
    crate::prism::parse(source, cx.target_ruby_version())
        .errors()
        .next()
        .is_none()
}

/// Items grouped by `reparse_scope`, in first-seen order.
fn scope_groups<'a, I: Copy, C: ReparsedEquivalence<I> + ?Sized>(
    cop: &C,
    cx: &Investigation<'a, '_>,
    items: &[I],
) -> Vec<(Option<Node<'a>>, Vec<I>)> {
    let mut groups: Vec<(Option<Node<'a>>, Vec<I>)> = Vec::new();
    for item in items {
        let scope = reparse_scope(cx, cop.item_range(*item));
        match groups.iter_mut().find(|(s, _)| *s == scope) {
            Some((_, group)) => group.push(*item),
            None => groups.push((scope, vec![*item])),
        }
    }
    groups
}

/// `reparse_scope(range)`: the innermost def, class, module or sclass
/// containing the range.
fn reparse_scope<'a>(cx: &Investigation<'a, '_>, range: Range) -> Option<Node<'a>> {
    let mut node = cx.processed_source.ast();
    let mut scope = None;
    while let Some(current) = node {
        if current.is_any_def()
            || current.is_any(&[NodeType::Class, NodeType::Module, NodeType::Sclass])
        {
            scope = Some(current);
        }
        node = current
            .child_nodes()
            .find(|child| child.has_source() && child.source_range().contains(range));
    }
    scope
}

fn verification_too_large(cx: &Investigation<'_, '_>, scope: Option<Node<'_>>) -> bool {
    let size = match scope {
        Some(scope) => scope.source_range().len(),
        None => cx.processed_source.raw_source().len(),
    };
    size > MAX_VERIFICATION_FRAGMENT_SIZE
}

fn corrections_verify<I: Copy, C: ReparsedEquivalence<I> + ?Sized>(
    cop: &C,
    cx: &Investigation<'_, '_>,
    scope: Option<Node<'_>>,
    items: &[I],
) -> bool {
    let source = cx.processed_source.buffer_source();
    let mut corrector = Corrector::new(source.len());
    for item in items {
        cop.apply_reparse_correction(cx, &mut corrector, *item);
    }
    if corrector.is_clobbered() {
        return false;
    }
    let corrected = corrector.rewrite(source);
    match scope {
        Some(scope) => {
            let fragment = corrected_scope_fragment(source, scope, &corrected);
            match fragment {
                Some(fragment) => {
                    parses_equivalently(cop, cx, scope.source(), Some(scope), fragment)
                }
                None => false,
            }
        }
        None => parses_equivalently(cop, cx, source, cx.processed_source.ast(), &corrected),
    }
}

fn corrected_scope_fragment<'c>(
    source: &str,
    scope: Node<'_>,
    corrected: &'c str,
) -> Option<&'c str> {
    let delta = corrected.len() as isize - source.len() as isize;
    let range = scope.source_range();
    let end = (range.end_pos as isize + delta).max(range.begin_pos as isize) as usize;
    corrected.get(range.begin_pos..end.min(corrected.len()))
}

fn parses_equivalently<I: Copy, C: ReparsedEquivalence<I> + ?Sized>(
    cop: &C,
    cx: &Investigation<'_, '_>,
    original_source: &str,
    original_ast: Option<Node<'_>>,
    corrected_source: &str,
) -> bool {
    let path = cx.processed_source.path().display().to_string();
    let version = cx.target_ruby_version();

    // `normalized_original`.
    let original = cop.preprocess_reparsed_source(original_source);
    let reparsed_original;
    let original_root = if *original != *original_source || original_source.contains("__LINE__") {
        let result = crate::prism::parse(&original, version);
        reparsed_original = Ast::from_prism(&result, &original, &path);
        match reparsed_original.root() {
            Some(root) => Some(root),
            None => return false,
        }
    } else {
        original_ast
    };

    let rewritten_source = cop.preprocess_reparsed_source(corrected_source);
    let result = crate::prism::parse(&rewritten_source, version);
    if result.errors().next().is_some() {
        return false;
    }
    let rewritten = Ast::from_prism(&result, &rewritten_source, &path);
    match (original_root, rewritten.root()) {
        (Some(original), Some(rewritten)) => cop.reparsed_equal(original, rewritten),
        // `return false unless original_normalized`.
        _ => false,
    }
}
