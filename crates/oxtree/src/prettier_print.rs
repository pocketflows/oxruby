//! Port of prettier_print 1.2.1 (`lib/prettier_print.rb`), the Wadler-style
//! document printer that syntax_tree is built on.
//!
//! The print tree lives in an arena so that, as in the Ruby original, groups
//! can be marked broken after they were inserted (`break_parent` walks the
//! group stack) and `remove_breaks` can rewrite contents in place.
//!
//! Two flavors of `text` exist upstream: `PrettierPrint#text` appends to a
//! `Text` node (used by the ERB formatter), while `SyntaxTree::Formatter#text`
//! pushes the string itself into the tree. Both are supported. Output can go
//! to a `String` buffer (`StringBuffer`) or an array of strings
//! (`ArrayBuffer`), whose `trim!` only strips unfrozen strings; a Ruby string
//! literal is frozen, a computed one is not, so every piece of text carries
//! that flag.

use std::borrow::Cow;

pub type DocId = u32;

pub const MODE_BREAK: u8 = 1;
pub const MODE_FLAT: u8 = 2;

/// A string pushed straight into the tree (`SyntaxTree::Formatter#text`).
#[derive(Debug, Clone)]
pub struct Piece {
    pub text: Cow<'static, str>,
    pub frozen: bool,
    /// `String#length`, i.e. the number of characters.
    pub width: usize,
}

#[derive(Debug, Clone)]
pub enum Doc {
    Str(Piece),
    Text {
        objects: Vec<Piece>,
        width: usize,
    },
    Group {
        contents: Vec<DocId>,
        brk: bool,
    },
    Breakable {
        separator: &'static str,
        width: usize,
        force: bool,
        indent: bool,
    },
    BreakParent,
    Trim,
    IfBreak {
        break_contents: Vec<DocId>,
        flat_contents: Vec<DocId>,
    },
    Indent {
        contents: Vec<DocId>,
    },
    Align {
        indent: i64,
        contents: Vec<DocId>,
    },
    LineSuffix {
        priority: i64,
        contents: Vec<DocId>,
    },
}

/// Which list of a container doc new nodes are appended to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Contents,
    Break,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    doc: DocId,
    slot: Slot,
}

/// The `force:` argument of `breakable`. `SkipBreakParent` is any truthy
/// value other than `true` (syntax_tree passes `:skip_break_parent`): the
/// newline is forced but the enclosing groups are not broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Force {
    No,
    Yes,
    SkipBreakParent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferKind {
    /// `Buffer::StringBuffer`: `trim!` strips any trailing spaces and tabs.
    String,
    /// `Buffer::ArrayBuffer`: `trim!` pops whitespace-only strings and strips
    /// the last string only if it is not frozen.
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMode {
    /// `PrettierPrint#text`: append to a `Text` node.
    TextNode,
    /// `SyntaxTree::Formatter#text`: push the string into the tree.
    Raw,
}

pub struct PrettierPrint {
    pub docs: Vec<Doc>,
    pub maxwidth: usize,
    groups: Vec<DocId>,
    target: Target,
    text_mode: TextMode,
    buffer_kind: BufferKind,
}

/// An output buffer (`PrettierPrint::Buffer`).
struct Buffer {
    kind: BufferKind,
    string: String,
    pieces: Vec<(Cow<'static, str>, bool)>,
}

impl Buffer {
    fn new(kind: BufferKind) -> Self {
        Buffer {
            kind,
            string: String::new(),
            pieces: Vec::new(),
        }
    }

    fn push(&mut self, text: Cow<'static, str>, frozen: bool) {
        match self.kind {
            BufferKind::String => self.string.push_str(&text),
            BufferKind::Array => self.pieces.push((text, frozen)),
        }
    }

    fn trim(&mut self) -> usize {
        match self.kind {
            BufferKind::String => {
                let trimmed = self.string.trim_end_matches([' ', '\t']).len();
                let removed = self.string.len() - trimmed;
                self.string.truncate(trimmed);
                removed
            }
            BufferKind::Array => {
                let mut trimmed = 0;
                while let Some((last, _)) = self.pieces.last() {
                    if last.chars().all(|c| c == ' ' || c == '\t') {
                        trimmed += last.chars().count();
                        self.pieces.pop();
                    } else {
                        break;
                    }
                }
                if let Some((last, frozen)) = self.pieces.last_mut()
                    && !*frozen
                {
                    let keep = last.trim_end_matches([' ', '\t']).len();
                    let removed = last.len() - keep;
                    if removed > 0 {
                        trimmed += removed;
                        last.to_mut().truncate(keep);
                    }
                }
                trimmed
            }
        }
    }

    fn into_string(self) -> String {
        match self.kind {
            BufferKind::String => self.string,
            BufferKind::Array => {
                let mut out = String::with_capacity(self.pieces.iter().map(|(p, _)| p.len()).sum());
                for (piece, _) in &self.pieces {
                    out.push_str(piece);
                }
                out
            }
        }
    }
}

type Command = (i64, u8, DocId);

impl PrettierPrint {
    pub fn new(maxwidth: usize, text_mode: TextMode, buffer_kind: BufferKind) -> Self {
        let mut q = PrettierPrint {
            docs: Vec::with_capacity(256),
            maxwidth,
            groups: Vec::new(),
            target: Target {
                doc: 0,
                slot: Slot::Contents,
            },
            text_mode,
            buffer_kind,
        };
        q.reset();
        q
    }

    fn reset(&mut self) {
        let root = self.alloc(Doc::Group {
            contents: Vec::new(),
            brk: false,
        });
        self.groups = vec![root];
        self.target = Target {
            doc: root,
            slot: Slot::Contents,
        };
    }

    fn alloc(&mut self, doc: Doc) -> DocId {
        self.docs.push(doc);
        (self.docs.len() - 1) as DocId
    }

    pub fn doc(&self, id: DocId) -> &Doc {
        &self.docs[id as usize]
    }

    fn list_mut(&mut self, target: Target) -> &mut Vec<DocId> {
        match (&mut self.docs[target.doc as usize], target.slot) {
            (Doc::Group { contents, .. }, Slot::Contents)
            | (Doc::Indent { contents }, Slot::Contents)
            | (Doc::Align { contents, .. }, Slot::Contents)
            | (Doc::LineSuffix { contents, .. }, Slot::Contents) => contents,
            (Doc::IfBreak { break_contents, .. }, Slot::Break) => break_contents,
            (Doc::IfBreak { flat_contents, .. }, Slot::Flat) => flat_contents,
            _ => unreachable!("invalid print target"),
        }
    }

    fn list(&self, target: Target) -> &Vec<DocId> {
        match (&self.docs[target.doc as usize], target.slot) {
            (Doc::Group { contents, .. }, Slot::Contents)
            | (Doc::Indent { contents }, Slot::Contents)
            | (Doc::Align { contents, .. }, Slot::Contents)
            | (Doc::LineSuffix { contents, .. }, Slot::Contents) => contents,
            (Doc::IfBreak { break_contents, .. }, Slot::Break) => break_contents,
            (Doc::IfBreak { flat_contents, .. }, Slot::Flat) => flat_contents,
            _ => unreachable!("invalid print target"),
        }
    }

    /// `target << doc`
    pub fn push(&mut self, doc: Doc) -> DocId {
        let id = self.alloc(doc);
        let target = self.target;
        self.list_mut(target).push(id);
        id
    }

    pub fn push_id(&mut self, id: DocId) {
        let target = self.target;
        self.list_mut(target).push(id);
    }

    fn with_target<R>(&mut self, target: Target, f: impl FnOnce(&mut Self) -> R) -> R {
        let previous = self.target;
        self.target = target;
        let result = f(self);
        self.target = previous;
        result
    }

    pub fn current_group_broken(&self) -> bool {
        let last = *self.groups.last().expect("group stack");
        matches!(self.docs[last as usize], Doc::Group { brk: true, .. })
    }

    fn group_break(&mut self, id: DocId) {
        if let Doc::Group { brk, .. } = &mut self.docs[id as usize] {
            *brk = true;
        }
    }

    fn group_is_broken(&self, id: DocId) -> bool {
        matches!(self.docs[id as usize], Doc::Group { brk: true, .. })
    }

    // ------------------------------------------------------------------
    // Markers node builders
    // ------------------------------------------------------------------

    /// `text`, in whichever flavor this printer was built with. `frozen`
    /// mirrors whether the Ruby string would be frozen (a literal).
    pub fn text_piece(&mut self, text: Cow<'static, str>, frozen: bool) {
        let width = text.chars().count();
        self.text_with_width(text, frozen, width);
    }

    pub fn text_with_width(&mut self, text: Cow<'static, str>, frozen: bool, width: usize) {
        let piece = Piece {
            text,
            frozen,
            width,
        };
        match self.text_mode {
            TextMode::Raw => {
                self.push(Doc::Str(piece));
            }
            TextMode::TextNode => {
                let target = self.target;
                let last = self.list(target).last().copied();
                if let Some(last) = last
                    && let Doc::Text { objects, width } = &mut self.docs[last as usize]
                {
                    *width += piece.width;
                    objects.push(piece);
                    return;
                }
                self.push(Doc::Text {
                    width: piece.width,
                    objects: vec![piece],
                });
            }
        }
    }

    /// A frozen string literal.
    pub fn text(&mut self, text: &'static str) {
        self.text_piece(Cow::Borrowed(text), true);
    }

    /// A computed (unfrozen) string.
    pub fn text_owned(&mut self, text: String) {
        self.text_piece(Cow::Owned(text), false);
    }

    pub fn breakable(&mut self, separator: &'static str, indent: bool, force: Force) {
        let width = separator.chars().count();
        self.breakable_with_width(separator, width, indent, force);
    }

    pub fn breakable_with_width(
        &mut self,
        separator: &'static str,
        width: usize,
        indent: bool,
        force: Force,
    ) {
        self.push(Doc::Breakable {
            separator,
            width,
            force: force != Force::No,
            indent,
        });
        if force == Force::Yes {
            self.break_parent();
        }
    }

    pub fn breakable_space(&mut self) {
        self.breakable(" ", true, Force::No);
    }

    pub fn breakable_empty(&mut self) {
        self.breakable("", true, Force::No);
    }

    pub fn breakable_force(&mut self) {
        self.push(Doc::Breakable {
            separator: " ",
            width: 1,
            force: true,
            indent: true,
        });
        self.break_parent();
    }

    pub fn breakable_return(&mut self) {
        self.push(Doc::Breakable {
            separator: " ",
            width: 1,
            force: true,
            indent: false,
        });
        self.break_parent();
    }

    pub fn comma_breakable(&mut self) {
        self.text(",");
        self.breakable_space();
    }

    pub fn break_parent(&mut self) {
        self.push(Doc::BreakParent);
        for index in (0..self.groups.len()).rev() {
            let group = self.groups[index];
            if self.group_is_broken(group) {
                break;
            }
            self.group_break(group);
        }
    }

    pub fn trim(&mut self) {
        self.push(Doc::Trim);
    }

    // ------------------------------------------------------------------
    // Container node builders
    // ------------------------------------------------------------------

    /// `SyntaxTree::Formatter#group` / `PrettierPrint#group` without the
    /// indent and open/close arguments.
    pub fn group<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (DocId, R) {
        let doc = self.push(Doc::Group {
            contents: Vec::new(),
            brk: false,
        });
        self.groups.push(doc);
        let result = self.with_target(
            Target {
                doc,
                slot: Slot::Contents,
            },
            f,
        );
        self.groups.pop();
        (doc, result)
    }

    /// `PrettierPrint#group(indent, open, close)`.
    pub fn group_with(
        &mut self,
        indent: i64,
        open: &'static str,
        close: &'static str,
        f: impl FnOnce(&mut Self),
    ) -> DocId {
        if !open.is_empty() {
            self.text(open);
        }
        let (doc, _) = self.group(|q| {
            if indent != 0 {
                q.nest(indent, f);
            } else {
                f(q);
            }
        });
        if !close.is_empty() {
            self.text(close);
        }
        doc
    }

    pub fn indent<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let doc = self.push(Doc::Indent {
            contents: Vec::new(),
        });
        self.with_target(
            Target {
                doc,
                slot: Slot::Contents,
            },
            f,
        )
    }

    pub fn nest<R>(&mut self, indent: i64, f: impl FnOnce(&mut Self) -> R) -> R {
        self.nest_doc(indent, f).1
    }

    pub fn nest_doc<R>(&mut self, indent: i64, f: impl FnOnce(&mut Self) -> R) -> (DocId, R) {
        let doc = self.push(Doc::Align {
            indent,
            contents: Vec::new(),
        });
        let result = self.with_target(
            Target {
                doc,
                slot: Slot::Contents,
            },
            f,
        );
        (doc, result)
    }

    pub fn line_suffix(&mut self, priority: i64, f: impl FnOnce(&mut Self)) {
        let doc = self.push(Doc::LineSuffix {
            priority,
            contents: Vec::new(),
        });
        self.with_target(
            Target {
                doc,
                slot: Slot::Contents,
            },
            f,
        );
    }

    /// `q.if_break { ... }.if_flat { ... }`. The flat block is built only if
    /// the enclosing group isn't already broken (`IfBreakBuilder`); otherwise
    /// it is built into a throwaway group whose breaks propagate
    /// (`IfFlatIgnore`).
    pub fn if_break(&mut self, break_fn: impl FnOnce(&mut Self), flat_fn: impl FnOnce(&mut Self)) {
        let doc = self.push(Doc::IfBreak {
            break_contents: Vec::new(),
            flat_contents: Vec::new(),
        });
        self.with_target(
            Target {
                doc,
                slot: Slot::Break,
            },
            break_fn,
        );
        if self.current_group_broken() {
            self.if_flat_ignore(flat_fn);
        } else {
            self.with_target(
                Target {
                    doc,
                    slot: Slot::Flat,
                },
                flat_fn,
            );
        }
    }

    /// `q.if_break { ... }` without an `if_flat`.
    pub fn if_break_only(&mut self, break_fn: impl FnOnce(&mut Self)) {
        self.if_break(break_fn, |_| {});
    }

    fn if_flat_ignore(&mut self, f: impl FnOnce(&mut Self)) {
        // A group that is never inserted into the tree; it only collects
        // whether a forced break happened while building the flat contents.
        let group = self.alloc(Doc::Group {
            contents: Vec::new(),
            brk: false,
        });
        self.with_target(
            Target {
                doc: group,
                slot: Slot::Contents,
            },
            f,
        );
        if self.group_is_broken(group) {
            self.break_parent();
        }
    }

    /// `q.if_flat { ... }`.
    pub fn if_flat(&mut self, f: impl FnOnce(&mut Self)) {
        if self.current_group_broken() {
            self.if_flat_ignore(f);
        } else {
            let doc = self.push(Doc::IfBreak {
                break_contents: Vec::new(),
                flat_contents: Vec::new(),
            });
            self.with_target(
                Target {
                    doc,
                    slot: Slot::Flat,
                },
                f,
            );
        }
    }

    // ------------------------------------------------------------------
    // Helper node builders
    // ------------------------------------------------------------------

    pub fn last_position(&self, node: DocId) -> usize {
        let mut queue: std::collections::VecDeque<DocId> = std::collections::VecDeque::new();
        queue.push_back(node);
        let mut width = 0;
        while let Some(doc) = queue.pop_front() {
            match &self.docs[doc as usize] {
                Doc::Str(piece) => width += piece.width,
                Doc::Group { contents, .. }
                | Doc::Indent { contents }
                | Doc::Align { contents, .. } => {
                    for &child in contents.iter().rev() {
                        queue.push_front(child);
                    }
                }
                Doc::Breakable { .. } => width = 0,
                Doc::IfBreak { break_contents, .. } => {
                    for &child in break_contents.iter().rev() {
                        queue.push_front(child);
                    }
                }
                Doc::Text { width: w, .. } => width += w,
                _ => {}
            }
        }
        width
    }

    pub fn remove_breaks(&mut self, node: DocId, replace: &'static str) {
        let mut queue: std::collections::VecDeque<DocId> = std::collections::VecDeque::new();
        queue.push_back(node);
        while let Some(doc) = queue.pop_front() {
            let (contents, is_if_break) = match &self.docs[doc as usize] {
                Doc::Align { contents, .. }
                | Doc::Indent { contents }
                | Doc::Group { contents, .. } => (contents.clone(), false),
                Doc::IfBreak { flat_contents, .. } => (flat_contents.clone(), true),
                _ => continue,
            };
            let mapped: Vec<DocId> = contents
                .into_iter()
                .map(|child| self.remove_breaks_with(child, replace))
                .collect();
            queue.extend(mapped.iter().copied());
            match &mut self.docs[doc as usize] {
                Doc::Align { contents, .. }
                | Doc::Indent { contents }
                | Doc::Group { contents, .. } => *contents = mapped,
                Doc::IfBreak { flat_contents, .. } if is_if_break => *flat_contents = mapped,
                _ => {}
            }
        }
    }

    fn remove_breaks_with(&mut self, doc: DocId, replace: &'static str) -> DocId {
        match &self.docs[doc as usize] {
            Doc::Breakable {
                separator,
                width,
                force,
                ..
            } => {
                let object = if *force { replace } else { separator };
                let width = *width;
                self.alloc(Doc::Text {
                    objects: vec![Piece {
                        text: Cow::Borrowed(object),
                        frozen: true,
                        width: object.chars().count(),
                    }],
                    width,
                })
            }
            Doc::IfBreak { flat_contents, .. } => {
                let contents = flat_contents.clone();
                self.alloc(Doc::Align {
                    indent: 0,
                    contents,
                })
            }
            _ => doc,
        }
    }

    // ------------------------------------------------------------------
    // Printing
    // ------------------------------------------------------------------

    fn push_contents(commands: &mut Vec<Command>, contents: &[DocId], indent: i64, mode: u8) {
        commands.extend(contents.iter().rev().map(|&part| (indent, mode, part)));
    }

    /// Flushes the tree into a string (`flush` followed by `output.join`).
    pub fn flush(&mut self, base_indentation: usize) -> String {
        let doc = self.groups[0];
        let mut buffer = Buffer::new(self.buffer_kind);
        let mut position: i64 = base_indentation as i64;
        if base_indentation > 0 {
            buffer.push(Cow::Owned(" ".repeat(base_indentation)), false);
        }

        let mut commands: Vec<Command> = vec![(base_indentation as i64, MODE_BREAK, doc)];
        let mut should_remeasure = false;
        let mut line_suffixes: Vec<Command> = Vec::new();

        while let Some((indent, mode, doc)) = commands.pop() {
            match &self.docs[doc as usize] {
                Doc::Str(piece) => {
                    buffer.push(piece.text.clone(), piece.frozen);
                    position += piece.width as i64;
                }
                Doc::Group { contents, brk } => {
                    if mode == MODE_FLAT && !should_remeasure {
                        let next_mode = if *brk { MODE_BREAK } else { MODE_FLAT };
                        Self::push_contents(&mut commands, contents, indent, next_mode);
                    } else {
                        should_remeasure = false;
                        if *brk {
                            Self::push_contents(&mut commands, contents, indent, MODE_BREAK);
                        } else {
                            let next_commands: Vec<Command> = contents
                                .iter()
                                .rev()
                                .map(|&part| (indent, MODE_FLAT, part))
                                .collect();
                            let remaining = self.maxwidth as i64 - position;
                            if self.fits(&next_commands, &commands, remaining) {
                                commands.extend(next_commands);
                            } else {
                                commands.extend(
                                    next_commands
                                        .into_iter()
                                        .map(|(indent, _, part)| (indent, MODE_BREAK, part)),
                                );
                            }
                        }
                    }
                }
                Doc::Breakable {
                    separator,
                    width,
                    force,
                    indent: doc_indent,
                } => {
                    if mode == MODE_FLAT {
                        if *force {
                            should_remeasure = true;
                        } else {
                            buffer.push(Cow::Borrowed(separator), true);
                            position += *width as i64;
                            // `next` skips the line-suffix check below.
                            continue;
                        }
                    }
                    if !line_suffixes.is_empty() {
                        commands.push((indent, mode, doc));
                        self.flush_line_suffixes(&mut commands, &mut line_suffixes);
                        continue;
                    } else if !*doc_indent {
                        buffer.push(Cow::Borrowed("\n"), true);
                        position = 0;
                    } else {
                        buffer.trim();
                        buffer.push(Cow::Borrowed("\n"), true);
                        buffer.push(Cow::Owned(" ".repeat(indent.max(0) as usize)), false);
                        position = indent;
                    }
                }
                Doc::Indent { contents } => {
                    Self::push_contents(&mut commands, contents, indent + 2, mode);
                }
                Doc::Align {
                    indent: align,
                    contents,
                } => {
                    Self::push_contents(&mut commands, contents, indent + align, mode);
                }
                Doc::Trim => {
                    position -= buffer.trim() as i64;
                }
                Doc::IfBreak {
                    break_contents,
                    flat_contents,
                } => {
                    if mode == MODE_BREAK && !break_contents.is_empty() {
                        Self::push_contents(&mut commands, break_contents, indent, mode);
                    } else if mode == MODE_FLAT && !flat_contents.is_empty() {
                        Self::push_contents(&mut commands, flat_contents, indent, mode);
                    }
                }
                Doc::LineSuffix { .. } => {
                    line_suffixes.push((indent, mode, doc));
                }
                Doc::BreakParent => {}
                Doc::Text { objects, width } => {
                    for object in objects {
                        buffer.push(object.text.clone(), object.frozen);
                    }
                    position += *width as i64;
                }
            }

            if commands.is_empty() && !line_suffixes.is_empty() {
                self.flush_line_suffixes(&mut commands, &mut line_suffixes);
            }
        }

        self.reset();
        buffer.into_string()
    }

    fn flush_line_suffixes(&self, commands: &mut Vec<Command>, line_suffixes: &mut Vec<Command>) {
        // sort_by { [-priority, -index] }
        let mut ordered: Vec<(usize, Command)> =
            line_suffixes.iter().copied().enumerate().collect();
        ordered.sort_by(|(ia, a), (ib, b)| {
            let pa = self.line_suffix_priority(a.2);
            let pb = self.line_suffix_priority(b.2);
            pb.cmp(&pa).then(ib.cmp(ia))
        });
        for (_, (indent, mode, doc)) in ordered {
            if let Doc::LineSuffix { contents, .. } = &self.docs[doc as usize] {
                Self::push_contents(commands, contents, indent, mode);
            }
        }
        line_suffixes.clear();
    }

    fn line_suffix_priority(&self, doc: DocId) -> i64 {
        match &self.docs[doc as usize] {
            Doc::LineSuffix { priority, .. } => *priority,
            _ => 0,
        }
    }

    fn fits(
        &self,
        next_commands: &[Command],
        rest_commands: &[Command],
        mut remaining: i64,
    ) -> bool {
        let mut rest_index = rest_commands.len();
        let mut commands: Vec<Command> = next_commands.to_vec();
        let mut fit_buffer = Buffer::new(self.buffer_kind);

        while remaining >= 0 {
            let Some((indent, mode, doc)) = commands.pop() else {
                if rest_index == 0 {
                    return true;
                }
                rest_index -= 1;
                commands.push(rest_commands[rest_index]);
                continue;
            };

            match &self.docs[doc as usize] {
                Doc::Str(piece) => {
                    fit_buffer.push(piece.text.clone(), piece.frozen);
                    remaining -= piece.width as i64;
                }
                Doc::Group { contents, brk } => {
                    let next_mode = if *brk { MODE_BREAK } else { mode };
                    Self::push_contents(&mut commands, contents, indent, next_mode);
                }
                Doc::Breakable {
                    separator,
                    width,
                    force,
                    ..
                } => {
                    if mode == MODE_FLAT && !*force {
                        fit_buffer.push(Cow::Borrowed(separator), true);
                        remaining -= *width as i64;
                        continue;
                    }
                    return true;
                }
                Doc::Indent { contents } => {
                    Self::push_contents(&mut commands, contents, indent + 2, mode);
                }
                Doc::Align {
                    indent: align,
                    contents,
                } => {
                    Self::push_contents(&mut commands, contents, indent + align, mode);
                }
                Doc::Trim => {
                    remaining += fit_buffer.trim() as i64;
                }
                Doc::IfBreak {
                    break_contents,
                    flat_contents,
                } => {
                    if mode == MODE_BREAK && !break_contents.is_empty() {
                        Self::push_contents(&mut commands, break_contents, indent, mode);
                    } else if mode == MODE_FLAT && !flat_contents.is_empty() {
                        Self::push_contents(&mut commands, flat_contents, indent, mode);
                    }
                }
                Doc::Text { objects, width } => {
                    for object in objects {
                        fit_buffer.push(object.text.clone(), object.frozen);
                    }
                    remaining -= *width as i64;
                }
                Doc::LineSuffix { .. } | Doc::BreakParent => {}
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printer(width: usize) -> PrettierPrint {
        PrettierPrint::new(width, TextMode::TextNode, BufferKind::String)
    }

    #[test]
    fn breaks_groups_that_do_not_fit() {
        let mut q = printer(10);
        q.group(|q| {
            q.text("hello");
            q.breakable_space();
            q.text("world!");
        });
        assert_eq!(q.flush(0), "hello\nworld!");

        let mut q = printer(20);
        q.group(|q| {
            q.text("hello");
            q.breakable_space();
            q.text("world!");
        });
        assert_eq!(q.flush(0), "hello world!");
    }

    #[test]
    fn indents_and_trims() {
        let mut q = PrettierPrint::new(80, TextMode::Raw, BufferKind::Array);
        q.text("def foo");
        q.indent(|q| {
            q.breakable_force();
            q.text_owned("# trailing   ".to_string());
        });
        q.breakable_force();
        q.text("end");
        assert_eq!(q.flush(0), "def foo\n  # trailing\nend");
    }

    #[test]
    fn line_suffixes_flush_before_newlines() {
        let mut q = PrettierPrint::new(80, TextMode::Raw, BufferKind::Array);
        q.text("a");
        q.line_suffix(1, |q| q.text(" # c"));
        q.text("b");
        q.breakable_force();
        q.text("c");
        assert_eq!(q.flush(0), "ab # c\nc");
    }
}

/// How the flat half of an `if_break` (or an `if_flat`) is being built.
#[derive(Debug, Clone, Copy)]
pub enum FlatMode {
    /// Into the IfBreak's flat contents (`IfBreakBuilder`).
    Contents(Target),
    /// Into a throwaway group whose breaks propagate (`IfFlatIgnore`).
    Ignore { group: DocId, previous: Target },
}

impl PrettierPrint {
    pub fn target(&self) -> Target {
        self.target
    }

    pub fn restore(&mut self, previous: Target) {
        self.target = previous;
    }

    pub fn open_group(&mut self) -> (DocId, Target) {
        let doc = self.push(Doc::Group {
            contents: Vec::new(),
            brk: false,
        });
        self.groups.push(doc);
        let previous = self.target;
        self.target = Target {
            doc,
            slot: Slot::Contents,
        };
        (doc, previous)
    }

    pub fn close_group(&mut self, previous: Target) {
        self.groups.pop();
        self.target = previous;
    }

    pub fn open_indent(&mut self) -> Target {
        let doc = self.push(Doc::Indent {
            contents: Vec::new(),
        });
        let previous = self.target;
        self.target = Target {
            doc,
            slot: Slot::Contents,
        };
        previous
    }

    pub fn open_align(&mut self, indent: i64) -> (DocId, Target) {
        let doc = self.push(Doc::Align {
            indent,
            contents: Vec::new(),
        });
        let previous = self.target;
        self.target = Target {
            doc,
            slot: Slot::Contents,
        };
        (doc, previous)
    }

    pub fn open_line_suffix(&mut self, priority: i64) -> Target {
        let doc = self.push(Doc::LineSuffix {
            priority,
            contents: Vec::new(),
        });
        let previous = self.target;
        self.target = Target {
            doc,
            slot: Slot::Contents,
        };
        previous
    }

    /// Pushes an IfBreak and targets its break contents.
    pub fn open_if_break(&mut self) -> (DocId, Target) {
        let doc = self.push(Doc::IfBreak {
            break_contents: Vec::new(),
            flat_contents: Vec::new(),
        });
        let previous = self.target;
        self.target = Target {
            doc,
            slot: Slot::Break,
        };
        (doc, previous)
    }

    /// After the break contents of `doc` were built (and the target restored),
    /// starts building its flat contents.
    pub fn begin_flat_of(&mut self, doc: DocId) -> FlatMode {
        let previous = self.target;
        if self.current_group_broken() {
            let group = self.alloc(Doc::Group {
                contents: Vec::new(),
                brk: false,
            });
            self.target = Target {
                doc: group,
                slot: Slot::Contents,
            };
            FlatMode::Ignore { group, previous }
        } else {
            self.target = Target {
                doc,
                slot: Slot::Flat,
            };
            FlatMode::Contents(previous)
        }
    }

    /// `if_flat` without a preceding `if_break`.
    pub fn begin_if_flat(&mut self) -> FlatMode {
        if self.current_group_broken() {
            let previous = self.target;
            let group = self.alloc(Doc::Group {
                contents: Vec::new(),
                brk: false,
            });
            self.target = Target {
                doc: group,
                slot: Slot::Contents,
            };
            FlatMode::Ignore { group, previous }
        } else {
            let doc = self.push(Doc::IfBreak {
                break_contents: Vec::new(),
                flat_contents: Vec::new(),
            });
            let previous = self.target;
            self.target = Target {
                doc,
                slot: Slot::Flat,
            };
            FlatMode::Contents(previous)
        }
    }

    pub fn end_flat(&mut self, mode: FlatMode) {
        match mode {
            FlatMode::Contents(previous) => self.target = previous,
            FlatMode::Ignore { group, previous } => {
                self.target = previous;
                if self.group_is_broken(group) {
                    self.break_parent();
                }
            }
        }
    }
}

/// The node-builder half of `PrettierPrint`, with the block-taking methods
/// expressed as closures over the implementor so that formatters can call
/// back into themselves while building.
pub trait DocBuilder: Sized {
    fn pp(&mut self) -> &mut PrettierPrint;

    fn group<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> DocId {
        let (doc, previous) = self.pp().open_group();
        f(self);
        self.pp().close_group(previous);
        doc
    }

    fn indent(&mut self, f: impl FnOnce(&mut Self)) {
        let previous = self.pp().open_indent();
        f(self);
        self.pp().restore(previous);
    }

    fn nest(&mut self, indent: i64, f: impl FnOnce(&mut Self)) -> DocId {
        let (doc, previous) = self.pp().open_align(indent);
        f(self);
        self.pp().restore(previous);
        doc
    }

    fn line_suffix(&mut self, priority: i64, f: impl FnOnce(&mut Self)) {
        let previous = self.pp().open_line_suffix(priority);
        f(self);
        self.pp().restore(previous);
    }

    fn if_break(&mut self, break_fn: impl FnOnce(&mut Self), flat_fn: impl FnOnce(&mut Self)) {
        let (doc, previous) = self.pp().open_if_break();
        break_fn(self);
        self.pp().restore(previous);
        let mode = self.pp().begin_flat_of(doc);
        flat_fn(self);
        self.pp().end_flat(mode);
    }

    fn if_break_only(&mut self, break_fn: impl FnOnce(&mut Self)) {
        let (_, previous) = self.pp().open_if_break();
        break_fn(self);
        self.pp().restore(previous);
    }

    fn if_flat(&mut self, f: impl FnOnce(&mut Self)) {
        let mode = self.pp().begin_if_flat();
        f(self);
        self.pp().end_flat(mode);
    }

    fn text(&mut self, text: &'static str) {
        self.pp().text(text);
    }

    fn text_owned(&mut self, text: String) {
        self.pp().text_owned(text);
    }

    fn breakable_space(&mut self) {
        self.pp().breakable_space();
    }

    fn breakable_empty(&mut self) {
        self.pp().breakable_empty();
    }

    fn breakable_force(&mut self) {
        self.pp().breakable_force();
    }

    fn breakable_return(&mut self) {
        self.pp().breakable_return();
    }

    fn breakable(&mut self, separator: &'static str, indent: bool, force: Force) {
        self.pp().breakable(separator, indent, force);
    }

    fn comma_breakable(&mut self) {
        self.pp().comma_breakable();
    }

    fn break_parent(&mut self) {
        self.pp().break_parent();
    }

    fn trim(&mut self) {
        self.pp().trim();
    }
}
