//! Port of `lib/rubocop/cop/lint/out_of_range_regexp_ref.rb`.

use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};
use crate::ext::regexp_node::{each_capture, parsed_tree};

const MSG: &str = "$%<backref>s is out of range (%<count>s regexp capture %<group>s detected).";

const REGEXP_ARGUMENT_METHODS: &[&str] = &[
    "=~",
    "match",
    "grep",
    "gsub",
    "gsub!",
    "sub",
    "sub!",
    "[]",
    "slice",
    "slice!",
    "index",
    "rindex",
    "scan",
    "partition",
    "rpartition",
    "start_with?",
    "end_with?",
];
/// `REGEXP_RECEIVER_METHODS` (`=~ === match`) + `REGEXP_ARGUMENT_METHODS`.
const REGEXP_CAPTURE_METHODS: &[&str] = &[
    "=~",
    "===",
    "match",
    "grep",
    "gsub",
    "gsub!",
    "sub",
    "sub!",
    "[]",
    "slice",
    "slice!",
    "index",
    "rindex",
    "scan",
    "partition",
    "rpartition",
    "start_with?",
    "end_with?",
];

#[derive(Default)]
pub struct OutOfRangeRegexpRef {
    valid_ref: Option<usize>,
}

impl Cop for OutOfRangeRegexpRef {
    fn name(&self) -> &'static str {
        "Lint/OutOfRangeRegexpRef"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[
            NodeType::MatchWithLvasgn,
            NodeType::When,
            NodeType::InPattern,
            NodeType::NthRef,
        ]
    }

    fn after_node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        REGEXP_CAPTURE_METHODS
    }

    fn on_new_investigation(&mut self, _cx: &mut Investigation<'_, '_>) {
        self.valid_ref = Some(0);
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        match node.ty() {
            NodeType::MatchWithLvasgn => {
                if let Some(regexp) = node.child(0) {
                    self.check_regexp(regexp);
                }
            }
            NodeType::When => {
                let regexp_conditions = node
                    .conditions()
                    .into_iter()
                    .filter(|condition| condition.is(NodeType::Regexp));
                self.valid_ref = regexp_conditions
                    .filter_map(|condition| self.check_regexp(condition))
                    .max();
            }
            NodeType::InPattern => {
                let regexp_patterns = regexp_patterns(node);
                self.valid_ref = regexp_patterns
                    .into_iter()
                    .filter_map(|pattern| self.check_regexp(pattern))
                    .max();
            }
            NodeType::NthRef => self.on_nth_ref(cx, node),
            _ => {}
        }
    }

    fn after_node<'a>(&mut self, _cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        if backrefs_unaffected(node) {
            return;
        }
        self.valid_ref = None;
        if regexp_first_argument(node) {
            self.check_regexp(node.first_argument().unwrap());
        } else if regexp_receiver(node) {
            self.check_regexp(node.receiver().unwrap());
        }
    }
}

impl OutOfRangeRegexpRef {
    fn on_nth_ref(&self, cx: &mut Investigation<'_, '_>, node: Node<'_>) {
        let Some(valid_ref) = self.valid_ref else {
            return;
        };
        let backref = match node.children().first() {
            Some(Child::Int(n)) => {
                if usize::try_from(*n).is_ok_and(|n| n <= valid_ref) {
                    return;
                }
                n.to_string()
            }
            Some(Child::BigInt(n)) => n.clone(),
            _ => return,
        };
        let count = if valid_ref == 0 {
            "no".to_owned()
        } else {
            valid_ref.to_string()
        };
        let group = if valid_ref == 1 { "group" } else { "groups" };
        let message = MSG
            .replace("%<backref>s", &backref)
            .replace("%<count>s", &count)
            .replace("%<group>s", group);
        cx.add_offense(node.source_range(), &message);
    }

    /// Returns the new `@valid_ref`, or `None` (leaving it unchanged) for
    /// an interpolated regexp.
    fn check_regexp(&mut self, node: Node<'_>) -> Option<usize> {
        // `interpolation?`
        if node.child_nodes().any(|child| child.is(NodeType::Begin)) {
            return None;
        }
        let tree = parsed_tree(node);
        let count = |named| tree.as_ref().map_or(0, |tree| each_capture(tree, Some(named)).len());
        let named_capture = count(true);
        let valid_ref = if named_capture > 0 {
            named_capture
        } else {
            count(false)
        };
        self.valid_ref = Some(valid_ref);
        Some(valid_ref)
    }
}

fn regexp_patterns(in_node: Node<'_>) -> Vec<Node<'_>> {
    let Some(pattern) = in_node.child(0) else {
        return Vec::new();
    };
    if pattern.is(NodeType::Regexp) {
        vec![pattern]
    } else {
        pattern
            .descendants()
            .filter(|d| d.is(NodeType::Regexp))
            .collect()
    }
}

/// A call such as `hash[:key]` or `array[0]` cannot involve a regexp match:
/// a non-string literal cannot act as a pattern (whereas `sub`, `gsub`,
/// and `match` treat a string one as a pattern and do set backreferences),
/// and `$~` is frame-local, so a user-defined method cannot change
/// the caller's backreferences either.
fn backrefs_unaffected(send_node: Node<'_>) -> bool {
    let Some(first_argument) = send_node.first_argument() else {
        return false;
    };
    if !first_argument.is_basic_literal() {
        return false;
    }
    if first_argument.is(NodeType::Str) {
        return false;
    }
    !regexp_receiver(send_node)
}

fn regexp_first_argument(send_node: Node<'_>) -> bool {
    send_node
        .first_argument()
        .is_some_and(|a| a.is(NodeType::Regexp))
        && send_node
            .method_name()
            .is_some_and(|m| REGEXP_ARGUMENT_METHODS.contains(&m))
}

fn regexp_receiver(send_node: Node<'_>) -> bool {
    send_node.receiver().is_some_and(|r| r.is(NodeType::Regexp))
}
