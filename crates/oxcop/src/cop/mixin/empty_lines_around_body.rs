//! Port of `lib/rubocop/cop/mixin/empty_lines_around_body.rb`.
//!
//! The including cop's `KIND` and `style` are passed in: `check` takes the
//! cop's kind (`'class'`, `` '`begin`' ``, ...) and its style, which some
//! cops fix (`EmptyLinesAroundBeginBody` is always `no_empty_lines`).

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::Investigation;
use crate::cop::correctors::empty_line_corrector::{self, OffenseStyle};
use crate::cop::mixin::range_help::source_range;
use crate::cop::mixin::util::comment_line;

pub const MSG_EXTRA: &str = "Extra empty line detected at %<kind>s body %<location>s.";
pub const MSG_MISSING: &str = "Empty line missing at %<kind>s body %<location>s.";
const MSG_DEFERRED: &str = "Empty line missing before first %<type>s definition";

static CONSTANT_DEFINITION: NodePattern = NodePattern::new("{class module}");
static EMPTY_LINE_REQUIRED: NodePattern =
    NodePattern::new("{any_def class module (send nil? {:private :protected :public})}");

/// The including cop's `KIND` and `style`.
#[derive(Debug, Clone, Copy)]
pub struct Body<'s> {
    pub kind: &'static str,
    pub style: &'s str,
}

impl Body<'_> {
    /// `check(node, body, adjusted_first_line:)`.
    pub fn check(
        &self,
        cx: &mut Investigation<'_, '_>,
        node: Node<'_>,
        body: Option<Node<'_>>,
        adjusted_first_line: Option<usize>,
    ) {
        if self.valid_body_style(body) || node.is_single_line() {
            return;
        }
        let first_line = adjusted_first_line.unwrap_or(node.first_line());
        let last_line = node.last_line();
        match self.style {
            "empty_lines_except_namespace" => {
                self.check_empty_lines_except_namespace(cx, body, first_line, last_line)
            }
            "empty_lines_special" => {
                self.check_empty_lines_special(cx, body, first_line, last_line)
            }
            style => self.check_both(cx, style, first_line, last_line),
        }
    }

    fn check_empty_lines_except_namespace(
        &self,
        cx: &mut Investigation<'_, '_>,
        body: Option<Node<'_>>,
        first_line: usize,
        last_line: usize,
    ) {
        if body.is_some_and(|body| namespace(body, true)) {
            self.check_both(cx, "no_empty_lines", first_line, last_line);
        } else {
            self.check_both(cx, "empty_lines", first_line, last_line);
        }
    }

    fn check_empty_lines_special(
        &self,
        cx: &mut Investigation<'_, '_>,
        body: Option<Node<'_>>,
        first_line: usize,
        last_line: usize,
    ) {
        let Some(body) = body else {
            return;
        };
        if namespace(body, true) {
            self.check_both(cx, "no_empty_lines", first_line, last_line);
        } else {
            if first_child_requires_empty_line(body) {
                self.check_beginning(cx, "empty_lines", first_line);
            } else {
                self.check_beginning(cx, "no_empty_lines", first_line);
                self.check_deferred_empty_line(cx, body);
            }
            self.check_ending(cx, "empty_lines", last_line);
        }
    }

    fn check_both(
        &self,
        cx: &mut Investigation<'_, '_>,
        style: &str,
        first_line: usize,
        last_line: usize,
    ) {
        match style {
            "beginning_only" => {
                self.check_beginning(cx, "empty_lines", first_line);
                self.check_ending(cx, "no_empty_lines", last_line);
            }
            "ending_only" => {
                self.check_beginning(cx, "no_empty_lines", first_line);
                self.check_ending(cx, "empty_lines", last_line);
            }
            _ => {
                self.check_beginning(cx, style, first_line);
                self.check_ending(cx, style, last_line);
            }
        }
    }

    fn check_beginning(&self, cx: &mut Investigation<'_, '_>, style: &str, first_line: usize) {
        self.check_source(cx, style, first_line as isize, "beginning");
    }

    fn check_ending(&self, cx: &mut Investigation<'_, '_>, style: &str, last_line: usize) {
        self.check_source(cx, style, last_line as isize - 2, "end");
    }

    fn check_source(
        &self,
        cx: &mut Investigation<'_, '_>,
        style: &str,
        line_no: isize,
        desc: &str,
    ) {
        match style {
            "no_empty_lines" => {
                let msg = self.message(MSG_EXTRA, desc);
                check_line(cx, style, line_no, &msg, |line| line.is_empty());
            }
            "empty_lines" => {
                let msg = self.message(MSG_MISSING, desc);
                check_line(cx, style, line_no, &msg, |line| !line.is_empty());
            }
            _ => {}
        }
    }

    fn check_deferred_empty_line(&self, cx: &mut Investigation<'_, '_>, body: Node<'_>) {
        let Some(node) = first_empty_line_required_child(body) else {
            return;
        };
        let ps = cx.processed_source;
        let line = previous_line_ignoring_comments(cx, node.first_line());
        if ps.lines().get(line).is_some_and(|l| l.is_empty()) {
            return;
        }
        let range = source_range(ps, line + 2, 0, 1);
        let message = MSG_DEFERRED.replace("%<type>s", node.ty().name());
        cx.add_offense_with_correction(range, &message, |corrector| {
            empty_line_corrector::correct(corrector, OffenseStyle::EmptyLines, range);
        });
    }

    /// `message(type, desc)`.
    pub fn message(&self, template: &str, desc: &str) -> String {
        template
            .replace("%<kind>s", self.kind)
            .replace("%<location>s", desc)
    }

    fn valid_body_style(&self, body: Option<Node<'_>>) -> bool {
        body.is_none() && self.style != "no_empty_lines"
    }
}

pub fn check_line(
    cx: &mut Investigation<'_, '_>,
    style: &str,
    line: isize,
    msg: &str,
    predicate: impl Fn(&str) -> bool,
) {
    let ps = cx.processed_source;
    // `processed_source.lines[line]`: a negative index counts from the end.
    let lines = ps.lines();
    let index = if line < 0 {
        lines.len() as isize + line
    } else {
        line
    };
    let Some(source_line) = usize::try_from(index).ok().and_then(|i| lines.get(i)) else {
        return;
    };
    if !predicate(source_line) {
        return;
    }
    let offset = if style == "empty_lines" && msg.contains("end.") {
        2
    } else {
        1
    };
    let range = source_range(ps, (line + offset) as usize, 0, 1);
    let style = if style == "empty_lines" {
        OffenseStyle::EmptyLines
    } else {
        OffenseStyle::NoEmptyLines
    };
    cx.add_offense_with_correction(range, msg, |corrector| {
        empty_line_corrector::correct(corrector, style, range);
    });
}

fn namespace(body: Node<'_>, with_one_child: bool) -> bool {
    if body.is(NodeType::Begin) {
        if with_one_child {
            return false;
        }
        body.child_nodes()
            .all(|child| CONSTANT_DEFINITION.is_match(child))
    } else {
        CONSTANT_DEFINITION.is_match(body)
    }
}

fn first_child_requires_empty_line(body: Node<'_>) -> bool {
    if body.is(NodeType::Begin) {
        body.child(0)
            .is_some_and(|c| EMPTY_LINE_REQUIRED.is_match(c))
    } else {
        EMPTY_LINE_REQUIRED.is_match(body)
    }
}

fn first_empty_line_required_child(body: Node<'_>) -> Option<Node<'_>> {
    if body.is(NodeType::Begin) {
        body.child_nodes()
            .find(|child| EMPTY_LINE_REQUIRED.is_match(*child))
    } else {
        EMPTY_LINE_REQUIRED.is_match(body).then_some(body)
    }
}

fn previous_line_ignoring_comments(cx: &Investigation<'_, '_>, send_line: usize) -> usize {
    let lines = cx.processed_source.lines();
    let mut line = send_line as isize - 2;
    while line >= 0 {
        if !comment_line(lines[line as usize]) {
            return line as usize;
        }
        line -= 1;
    }
    0
}
