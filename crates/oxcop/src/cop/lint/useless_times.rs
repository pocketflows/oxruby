//! Port of `lib/rubocop/cop/lint/useless_times.rb`.

use crate::ast::node_pattern::{MatchContext, NodePattern, Param};
use crate::ast::{Node, NodeType};
use crate::cop::mixin::range_help::range_by_whole_lines;
use crate::cop::{Corrector, Cop, Investigation};
use crate::processed_source::ProcessedSource;

static TIMES_CALL: NodePattern =
    NodePattern::new("(send (int $_) :times (block-pass (sym $_))?)");
static BLOCK_ARG: NodePattern = NodePattern::new("(block _ (args (arg $_)) ...)");
static BLOCK_REASSIGNS_ARG: NodePattern = NodePattern::new("(lvasgn %)");

#[derive(Default)]
pub struct UselessTimes;

impl Cop for UselessTimes {
    fn name(&self) -> &'static str {
        "Lint/UselessTimes"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["times"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = TIMES_CALL.match_node(node) else {
            return;
        };
        let Some(count) = captures[0].val().and_then(|v| v.int()) else {
            return;
        };
        let proc_name = captures[1].list().iter().find_map(|c| c.sym());
        if count > 1 {
            return;
        }

        let node = if node.is_block_literal() {
            node.block_node().unwrap_or(node)
        } else {
            node
        };
        let message = format!("Useless call to `{count}.times` detected.");
        let ps = cx.processed_source;
        cx.add_offense_with_correction(node.source_range(), &message, |corrector| {
            if !own_line(ps, node) || node.parent().is_some_and(|p| p.is(NodeType::Send)) {
                return;
            }
            autocorrect(ps, corrector, count, node, proc_name);
        });
    }
}

fn autocorrect(
    ps: &ProcessedSource<'_>,
    corrector: &mut Corrector,
    count: i64,
    node: Node<'_>,
    proc_name: Option<&str>,
) {
    if never_process(count, node) {
        corrector.remove(range_by_whole_lines(ps, node.source_range(), true));
    } else if let Some(proc_name) = proc_name {
        corrector.replace(node.source_range(), proc_name);
    } else if node.is(NodeType::Block) {
        autocorrect_block(corrector, node);
    }
}

fn never_process(count: i64, node: Node<'_>) -> bool {
    count < 1 || (node.is(NodeType::Block) && node.body().is_none())
}

fn autocorrect_block(corrector: &mut Corrector, node: Node<'_>) {
    let block_arg = BLOCK_ARG.match_node(node).and_then(|c| c[0].sym());
    if !reducible_to_body(node, block_arg) {
        return;
    }
    let Some(body) = node.body() else {
        return;
    };

    let mut source = body.source().to_owned();
    if let Some(block_arg) = block_arg
        && let Some(re) = crate::cop::mixin::ruby_regex(&format!(r"\b{block_arg}\b"))
    {
        source = re.replace_all(&source, |_: &onig::Captures<'_>| "0".to_owned());
    }
    corrector.replace(
        node.source_range(),
        fix_indentation(&source, node.column(), body.column()),
    );
}

fn reducible_to_body(node: Node<'_>, block_arg: Option<&str>) -> bool {
    let arguments = node.arguments();
    if arguments.len() > 1 || orphans_loop_control_keyword(node) {
        return false;
    }
    if arguments.len() == 1 && block_arg.is_none() {
        return false;
    }
    let Some(block_arg) = block_arg else {
        return true;
    };
    let params = [Param::Sym(block_arg)];
    let ctx = MatchContext {
        params: &params,
        ..Default::default()
    };
    BLOCK_REASSIGNS_ARG.search_with(node, &ctx).is_empty()
}

fn orphans_loop_control_keyword(node: Node<'_>) -> bool {
    let Some(body) = node.body() else {
        return false;
    };
    body.each_node()
        .filter(|n| matches!(n.ty(), NodeType::Next | NodeType::Break | NodeType::Redo))
        .any(|control| {
            control
                .ancestors()
                .take_while(|ancestor| *ancestor != node)
                .all(|ancestor| {
                    !(ancestor.is_any_block()
                        || matches!(
                            ancestor.ty(),
                            NodeType::While | NodeType::Until | NodeType::For
                        ))
                })
        })
}

fn fix_indentation(source: &str, start: usize, end: usize) -> String {
    let source_lines: Vec<String> = source
        .split('\n')
        .enumerate()
        .map(|(i, line)| {
            if i == 0 || line.is_empty() {
                return line.to_owned();
            }
            let chars: Vec<char> = line.chars().collect();
            if start > chars.len() {
                return line.to_owned();
            }
            let stop = end.min(chars.len()).max(start);
            chars[..start].iter().chain(&chars[stop..]).collect()
        })
        .collect();
    // `split("\n")` drops trailing empty strings.
    let mut lines = source_lines;
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines.join("\n")
}

fn own_line(ps: &ProcessedSource<'_>, node: Node<'_>) -> bool {
    let line = ps.source_line(node.first_line());
    !line
        .chars()
        .take(node.column())
        .any(|c| !c.is_whitespace())
}
