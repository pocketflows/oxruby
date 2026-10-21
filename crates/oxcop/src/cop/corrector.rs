//! Port of `lib/rubocop/cop/corrector.rb` and the parser gem's
//! `Parser::Source::TreeRewriter` (with `TreeRewriter::Action`), which it
//! builds on. RuboCop's policies: different replacements, swallowed
//! insertions and crossing insertions raise a clobbering error; crossing
//! deletions are accepted.

use crate::processed_source::Range;

/// `Parser::ClobberingError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClobberingError;

type Result<T> = std::result::Result<T, ClobberingError>;

/// A range that may extend one past either end of the source (the root
/// action's all-encompassing range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    begin: i64,
    end: i64,
}

impl Span {
    fn from_range(range: Range) -> Self {
        Self {
            begin: range.begin_pos as i64,
            end: range.end_pos as i64,
        }
    }

    fn is_empty(self) -> bool {
        self.begin == self.end
    }

    fn join(self, other: Span) -> Span {
        Span {
            begin: self.begin.min(other.begin),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Debug, Clone)]
struct Action {
    range: Span,
    insert_before: String,
    replacement: Option<String>,
    insert_after: String,
    children: Vec<Action>,
}

impl Action {
    fn new(range: Span) -> Self {
        Self {
            range,
            insert_before: String::new(),
            replacement: None,
            insert_after: String::new(),
            children: Vec::new(),
        }
    }

    fn combine(&self, action: Action) -> Result<Action> {
        if action.is_empty() {
            return Ok(self.clone());
        }
        self.do_combine(action)
    }

    fn is_empty(&self) -> bool {
        self.insert_before.is_empty()
            && self.insert_after.is_empty()
            && self.children.is_empty()
            && self
                .replacement
                .as_ref()
                .is_none_or(|r| r.is_empty() && self.range.is_empty())
    }

    fn ordered_replacements(&self, out: &mut Vec<(Span, String)>) {
        let r = self.range;
        if !self.insert_before.is_empty() {
            out.push((
                Span {
                    begin: r.begin,
                    end: r.begin,
                },
                self.insert_before.clone(),
            ));
        }
        if let Some(replacement) = &self.replacement {
            out.push((r, replacement.clone()));
        }
        for child in &self.children {
            child.ordered_replacements(out);
        }
        if !self.insert_after.is_empty() {
            out.push((
                Span {
                    begin: r.end,
                    end: r.end,
                },
                self.insert_after.clone(),
            ));
        }
    }

    fn is_insertion(&self) -> bool {
        !self.insert_before.is_empty()
            || !self.insert_after.is_empty()
            || self.replacement.as_ref().is_some_and(|r| !r.is_empty())
    }

    /// `with(...)`: a copy with some attributes changed. Children are
    /// swallowed when the result has a replacement.
    fn with(
        &self,
        range: Option<Span>,
        children: Option<Vec<Action>>,
        insert_before: Option<String>,
        replacement: Option<Option<String>>,
        insert_after: Option<String>,
    ) -> Result<Action> {
        let replacement = replacement.unwrap_or_else(|| self.replacement.clone());
        let range = range.unwrap_or(self.range);
        let mut children = children.unwrap_or_else(|| self.children.clone());
        if replacement.is_some() {
            children = Self::swallow(children)?;
        }
        Ok(Action {
            range,
            insert_before: insert_before.unwrap_or_else(|| self.insert_before.clone()),
            replacement,
            insert_after: insert_after.unwrap_or_else(|| self.insert_after.clone()),
            children,
        })
    }

    fn with_children(&self, children: Vec<Action>) -> Result<Action> {
        self.with(None, Some(children), None, None, None)
    }

    fn do_combine(&self, action: Action) -> Result<Action> {
        if action.range == self.range {
            self.merge(action)
        } else {
            self.place_in_hierarchy(action)
        }
    }

    fn place_in_hierarchy(&self, action: Action) -> Result<Action> {
        let family = self.analyse_hierarchy(&action)?;
        if let Some(fusible) = family.fusible {
            let mut others = family.sibbling_left;
            others.extend(family.child.unwrap_or_default());
            others.extend(family.sibbling_right);
            self.fuse_deletions(action, fusible, others)
        } else {
            let extra_sibbling = if let Some(parent) = family.parent {
                parent.do_combine(action)?
            } else if let Some(child) = family.child {
                let action_children = action.children.clone();
                action
                    .with_children(child)?
                    .combine_children(action_children)?
            } else {
                action
            };
            let mut children = family.sibbling_left;
            children.push(extra_sibbling);
            children.extend(family.sibbling_right);
            self.with_children(children)
        }
    }

    fn combine_children(self, more_children: Vec<Action>) -> Result<Action> {
        let mut parent = self;
        for new_child in more_children {
            parent = parent.place_in_hierarchy(new_child)?;
        }
        Ok(parent)
    }

    fn fuse_deletions(
        &self,
        action: Action,
        fusible: Vec<Action>,
        other_sibblings: Vec<Action>,
    ) -> Result<Action> {
        let without_fusible = self.with_children(other_sibblings)?;
        let fused_range = fusible
            .iter()
            .fold(action.range, |range, f| range.join(f.range));
        let fused_deletion = action.with(Some(fused_range), None, None, None, None)?;
        without_fusible.do_combine(fused_deletion)
    }

    fn bsearch_child_index(&self, from: usize, pred: impl Fn(&Action) -> bool) -> usize {
        from + self.children[from..].partition_point(|child| !pred(child))
    }

    fn analyse_hierarchy(&self, action: &Action) -> Result<Family> {
        let r = action.range;
        let mut left_index = self.bsearch_child_index(0, |child| child.range.end > r.begin);
        let start = left_index.saturating_sub(1);
        let mut right_index = self.bsearch_child_index(start, |child| child.range.begin >= r.end);
        let center = right_index as i64 - left_index as i64;
        let mut parent = None;
        let mut fusible = None;
        let mut contained = None;
        match center {
            0 => {}
            -1 => {
                left_index -= 1;
                right_index += 1;
                parent = Some(self.children[left_index].clone());
            }
            _ => {
                let overlap_left = self.children[left_index].range.begin.cmp(&r.begin);
                let overlap_right = self.children[right_index - 1].range.end.cmp(&r.end);
                if center == 1 && overlap_left.is_le() && overlap_right.is_ge() {
                    parent = Some(self.children[left_index].clone());
                } else {
                    let mut c: Vec<Action> = self.children[left_index..right_index].to_vec();
                    let first = if overlap_left.is_lt() {
                        Some(c.remove(0))
                    } else {
                        None
                    };
                    let last = if overlap_right.is_gt() { c.pop() } else { None };
                    fusible = Self::check_fusible(action, first, last)?;
                    contained = Some(c);
                }
            }
        }
        Ok(Family {
            parent,
            sibbling_left: self.children[..left_index].to_vec(),
            sibbling_right: self.children[right_index..].to_vec(),
            fusible,
            child: contained,
        })
    }

    fn check_fusible(
        action: &Action,
        first: Option<Action>,
        last: Option<Action>,
    ) -> Result<Option<Vec<Action>>> {
        let fusible: Vec<Action> = first.into_iter().chain(last).collect();
        if fusible.is_empty() {
            return Ok(None);
        }
        for child in &fusible {
            // crossing_insertions raise; crossing_deletions are accepted.
            if action.is_insertion() || child.is_insertion() {
                return Err(ClobberingError);
            }
        }
        Ok(Some(fusible))
    }

    fn merge(&self, action: Action) -> Result<Action> {
        if let (Some(mine), Some(theirs)) = (&self.replacement, &action.replacement)
            && mine != theirs
        {
            return Err(ClobberingError);
        }
        let replacement = action
            .replacement
            .clone()
            .or_else(|| self.replacement.clone());
        let merged = self.with(
            None,
            None,
            Some(format!("{}{}", action.insert_before, self.insert_before)),
            Some(replacement),
            Some(format!("{}{}", self.insert_after, action.insert_after)),
        )?;
        merged.combine_children(action.children)
    }

    fn swallow(children: Vec<Action>) -> Result<Vec<Action>> {
        if children.iter().any(Action::is_insertion) {
            return Err(ClobberingError);
        }
        Ok(Vec::new())
    }
}

struct Family {
    parent: Option<Action>,
    sibbling_left: Vec<Action>,
    sibbling_right: Vec<Action>,
    fusible: Option<Vec<Action>>,
    child: Option<Vec<Action>>,
}

/// `RuboCop::Cop::Corrector`.
#[derive(Debug, Clone)]
pub struct Corrector {
    source_len: usize,
    root: Action,
    /// Set when an edit clashed with an earlier one of the same corrector.
    clobbered: bool,
}

impl Corrector {
    pub fn new(source_len: usize) -> Self {
        Self {
            source_len,
            root: Action::new(Span {
                begin: -1,
                end: source_len as i64 + 1,
            }),
            clobbered: false,
        }
    }

    pub fn is_clobbered(&self) -> bool {
        self.clobbered
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_empty()
    }

    fn combine(
        &mut self,
        range: Range,
        insert_before: &str,
        replacement: Option<&str>,
        insert_after: &str,
    ) -> Result<()> {
        assert!(
            range.end_pos <= self.source_len && range.begin_pos <= range.end_pos,
            "The range {}...{} is outside the bounds of the source",
            range.begin_pos,
            range.end_pos
        );
        let action = Action {
            range: Span::from_range(range),
            insert_before: insert_before.to_owned(),
            replacement: replacement.map(str::to_owned),
            insert_after: insert_after.to_owned(),
            children: Vec::new(),
        };
        self.root = self.root.combine(action)?;
        Ok(())
    }

    /// Like the Ruby methods, these raise (here: return) a clobbering error
    /// when the edit conflicts with an earlier one.
    pub fn try_replace(&mut self, range: Range, content: &str) -> Result<()> {
        self.combine(range, "", Some(content), "")
    }

    pub fn try_wrap(&mut self, range: Range, before: &str, after: &str) -> Result<()> {
        self.combine(range, before, None, after)
    }

    pub fn replace(&mut self, range: Range, content: impl AsRef<str>) {
        self.record(|c| c.try_replace(range, content.as_ref()));
    }

    pub fn remove(&mut self, range: Range) {
        self.replace(range, "");
    }

    pub fn insert_before(&mut self, range: Range, content: impl AsRef<str>) {
        let content = content.as_ref().to_owned();
        self.record(|c| c.try_wrap(range, &content, ""));
    }

    pub fn insert_after(&mut self, range: Range, content: impl AsRef<str>) {
        let content = content.as_ref().to_owned();
        self.record(|c| c.try_wrap(range, "", &content));
    }

    pub fn wrap(&mut self, range: Range, before: impl AsRef<str>, after: impl AsRef<str>) {
        let (before, after) = (before.as_ref().to_owned(), after.as_ref().to_owned());
        self.record(|c| c.try_wrap(range, &before, &after));
    }

    pub fn remove_preceding(&mut self, range: Range, size: usize) {
        self.remove(Range::new(range.begin_pos - size, range.begin_pos));
    }

    pub fn remove_leading(&mut self, range: Range, size: usize) {
        self.remove(Range::new(range.begin_pos, range.begin_pos + size));
    }

    pub fn remove_trailing(&mut self, range: Range, size: usize) {
        self.remove(Range::new(range.end_pos - size, range.end_pos));
    }

    /// Port of `Corrector#swap`.
    pub fn swap(&mut self, source: &str, range1: Range, range2: Range) {
        let s1 = source[range1.begin_pos..range1.end_pos].to_owned();
        let s2 = source[range2.begin_pos..range2.end_pos].to_owned();
        if range1.end_pos == range2.begin_pos {
            self.insert_before(range1, s2);
            self.remove(range2);
        } else if range2.end_pos == range1.begin_pos {
            self.insert_before(range2, s1);
            self.remove(range1);
        } else {
            self.replace(range1, s2);
            self.replace(range2, s1);
        }
    }

    /// Edits made inside a cop's correction block: a clobbering error there
    /// is an error in the cop (RuboCop reports it and drops the offense).
    fn record(&mut self, edit: impl FnOnce(&mut Self) -> Result<()>) {
        if edit(self).is_err() {
            self.clobbered = true;
        }
    }

    /// `merge!`: all or nothing.
    pub fn merge(&mut self, other: &Corrector) -> Result<()> {
        let mut root = self.root.clone();
        for child in &other.root.children {
            root = root.combine(child.clone())?;
        }
        // The root action itself never carries edits.
        self.root = root;
        Ok(())
    }

    /// `process`: applies the edits to the source.
    pub fn rewrite(&self, source: &str) -> String {
        let mut replacements = Vec::new();
        self.root.ordered_replacements(&mut replacements);
        let mut out = String::with_capacity(source.len());
        let mut last_end = 0usize;
        for (span, replacement) in replacements {
            let begin = span.begin.max(0) as usize;
            if begin > last_end {
                out.push_str(&source[last_end..begin]);
            }
            out.push_str(&replacement);
            last_end = span.end.max(0) as usize;
        }
        if last_end < source.len() {
            out.push_str(&source[last_end..]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(b: usize, e: usize) -> Range {
        Range::new(b, e)
    }

    #[test]
    fn applies_edits_in_order() {
        let mut corrector = Corrector::new(7);
        corrector.remove(r(5, 6));
        corrector.wrap(r(0, 1), "#{'", "'}");
        assert_eq!(corrector.rewrite("x = 0 \n"), "#{'x'} = 0\n");
    }

    #[test]
    fn rejects_overlapping_replacements() {
        let mut a = Corrector::new(10);
        a.replace(r(0, 3), "a");
        let mut b = Corrector::new(10);
        b.replace(r(2, 4), "b");
        assert_eq!(a.merge(&b), Err(ClobberingError));
    }

    #[test]
    fn merges_insertions_at_same_point() {
        let mut c = Corrector::new(3);
        c.insert_before(r(1, 2), "a");
        c.insert_before(r(1, 2), "b");
        c.insert_after(r(1, 2), "c");
        c.insert_after(r(1, 2), "d");
        assert_eq!(c.rewrite("xyz"), "xbaycdz");
    }

    #[test]
    fn nested_replacement_inside_deletion() {
        let mut c = Corrector::new(6);
        c.remove(r(0, 6));
        c.remove(r(1, 2));
        assert_eq!(c.rewrite("abcdef"), "");
    }

    #[test]
    fn crossing_deletions_are_fused() {
        let mut c = Corrector::new(6);
        c.remove(r(0, 3));
        c.remove(r(2, 5));
        assert_eq!(c.rewrite("abcdef"), "f");
    }
}
