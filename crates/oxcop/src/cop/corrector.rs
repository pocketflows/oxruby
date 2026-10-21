//! Port of `lib/rubocop/cop/corrector.rb`, which builds on
//! `Parser::Source::TreeRewriter`. Edits are recorded against byte ranges of
//! the original source and applied in one pass by `rewrite`.

use crate::processed_source::Range;

#[derive(Debug, Clone)]
struct Edit {
    range: Range,
    before: String,
    replacement: Option<String>,
    after: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClobberingError;

#[derive(Debug, Clone, Default)]
pub struct Corrector {
    edits: Vec<Edit>,
}

impl Corrector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn replace(&mut self, range: Range, content: impl Into<String>) {
        self.push(Edit {
            range,
            before: String::new(),
            replacement: Some(content.into()),
            after: String::new(),
        });
    }

    pub fn remove(&mut self, range: Range) {
        self.replace(range, "");
    }

    pub fn insert_before(&mut self, range: Range, content: impl Into<String>) {
        self.push(Edit {
            range,
            before: content.into(),
            replacement: None,
            after: String::new(),
        });
    }

    pub fn insert_after(&mut self, range: Range, content: impl Into<String>) {
        self.push(Edit {
            range,
            before: String::new(),
            replacement: None,
            after: content.into(),
        });
    }

    pub fn wrap(&mut self, range: Range, before: impl Into<String>, after: impl Into<String>) {
        self.push(Edit {
            range,
            before: before.into(),
            replacement: None,
            after: after.into(),
        });
    }

    fn push(&mut self, edit: Edit) {
        self.edits.push(edit);
    }

    /// Merges another corrector's edits, all or nothing.
    pub fn merge(&mut self, other: &Corrector) -> Result<(), ClobberingError> {
        for edit in &other.edits {
            if self.edits.iter().any(|existing| clobbers(existing, edit)) {
                return Err(ClobberingError);
            }
        }
        self.edits.extend(other.edits.iter().cloned());
        Ok(())
    }

    pub fn rewrite(&self, source: &str) -> String {
        // (position, ordering class, sequence, text, end of replaced range)
        let mut ops: Vec<(usize, u8, usize, &str, Option<usize>)> = Vec::new();
        for (seq, edit) in self.edits.iter().enumerate() {
            let Range { begin_pos, end_pos } = edit.range;
            if !edit.before.is_empty() {
                ops.push((begin_pos, 1, seq * 3, &edit.before, None));
            }
            if let Some(replacement) = &edit.replacement {
                ops.push((begin_pos, 2, seq * 3 + 1, replacement, Some(end_pos)));
            }
            if !edit.after.is_empty() {
                let class = if edit.range.is_empty() { 3 } else { 0 };
                ops.push((end_pos, class, seq * 3 + 2, &edit.after, None));
            }
        }
        ops.sort_by_key(|&(pos, class, seq, _, _)| (pos, class, seq));

        let mut out = String::with_capacity(source.len());
        let mut cursor = 0;
        for (pos, _, _, text, replaced_end) in ops {
            if pos > cursor {
                out.push_str(&source[cursor..pos]);
                cursor = pos;
            }
            if pos < cursor && replaced_end.is_none() {
                continue;
            }
            out.push_str(text);
            if let Some(end) = replaced_end {
                cursor = cursor.max(end);
            }
        }
        out.push_str(&source[cursor..]);
        out
    }
}

fn clobbers(a: &Edit, b: &Edit) -> bool {
    let overlap = a.range.begin_pos < b.range.end_pos && b.range.begin_pos < a.range.end_pos;
    match (&a.replacement, &b.replacement) {
        (Some(x), Some(y)) => (overlap || (a.range == b.range)) && !(a.range == b.range && x == y),
        (Some(_), None) => inside(b.range, a.range),
        (None, Some(_)) => inside(a.range, b.range),
        (None, None) => false,
    }
}

/// Whether an insertion at `insertion`'s ends would be swallowed by a
/// replacement of `replaced`.
fn inside(insertion: Range, replaced: Range) -> bool {
    let strictly = |pos: usize| replaced.begin_pos < pos && pos < replaced.end_pos;
    strictly(insertion.begin_pos) || strictly(insertion.end_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_edits_in_order() {
        let mut corrector = Corrector::new();
        corrector.remove(Range::new(5, 6));
        corrector.wrap(Range::new(0, 1), "#{'", "'}");
        assert_eq!(corrector.rewrite("x = 0 \n"), "#{'x'} = 0\n");
    }

    #[test]
    fn rejects_overlapping_replacements() {
        let mut a = Corrector::new();
        a.replace(Range::new(0, 3), "a");
        let mut b = Corrector::new();
        b.replace(Range::new(2, 4), "b");
        assert_eq!(a.merge(&b), Err(ClobberingError));
    }
}
