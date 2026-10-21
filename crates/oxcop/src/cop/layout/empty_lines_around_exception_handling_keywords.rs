//! Port of `lib/rubocop/cop/layout/empty_lines_around_exception_handling_keywords.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::mixin::empty_lines_around_body::check_line;
use crate::cop::{Cop, Investigation};
use crate::processed_source::Range;

const STYLE: &str = "no_empty_lines";

#[derive(Default)]
pub struct EmptyLinesAroundExceptionHandlingKeywords;

impl Cop for EmptyLinesAroundExceptionHandlingKeywords {
    fn name(&self) -> &'static str {
        "Layout/EmptyLinesAroundExceptionHandlingKeywords"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        use NodeType::*;
        &[Def, Defs, Block, Numblock, Kwbegin]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let body = if node.is(NodeType::Kwbegin) {
            node.child(0)
        } else {
            node.body()
        };
        check_body(cx, body, node.first_line());
    }
}

fn check_body(cx: &mut Investigation<'_, '_>, body: Option<Node<'_>>, line_of_def_or_kwbegin: usize) {
    let Some(body) = body else {
        return;
    };
    for loc in keyword_locations(Some(body)) {
        let line = cx.processed_source.line_of(loc.begin_pos);
        if line == line_of_def_or_kwbegin || last_body_and_end_on_same_line(cx, body) {
            continue;
        }
        let keyword = cx.processed_source.slice(loc);
        check_line(cx, STYLE, line as isize, &message("after", keyword), str::is_empty);
        check_line(cx, STYLE, line as isize - 2, &message("before", keyword), str::is_empty);
    }
}

fn last_body_and_end_on_same_line(cx: &Investigation<'_, '_>, body: Node<'_>) -> bool {
    let ps = cx.processed_source;
    let Some(end) = body.parent().and_then(|p| p.loc().end) else {
        return false;
    };
    let end_keyword_line = ps.line_of(end.begin_pos);
    if !body.is(NodeType::Rescue) {
        return body.last_line() == end_keyword_line;
    }
    let last_body_line = match body.loc().else_ {
        Some(else_) => ps.line_of(else_.begin_pos),
        None => body.resbody_branches().last().unwrap().first_line(),
    };
    last_body_line == end_keyword_line
}

fn message(location: &str, keyword: &str) -> String {
    format!("Extra empty line detected {location} the `{keyword}`.")
}

fn keyword_locations(node: Option<Node<'_>>) -> Vec<Range> {
    let Some(node) = node else {
        return Vec::new();
    };
    match node.ty() {
        NodeType::Rescue => node
            .loc()
            .else_
            .into_iter()
            .chain(node.resbody_branches().iter().filter_map(|b| b.loc().keyword))
            .collect(),
        NodeType::Ensure => {
            let mut locations: Vec<Range> = node.loc().keyword.into_iter().collect();
            locations.extend(keyword_locations(node.child(0)));
            locations
        }
        _ => Vec::new(),
    }
}
