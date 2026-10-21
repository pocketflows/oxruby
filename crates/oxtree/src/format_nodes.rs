// Included from format.rs: the larger `format` methods of node.rb and the
// formatter objects they use (CallChainFormatter, ConditionalFormatter,
// FlowControlFormatter, LoopFormatter, HashKeyFormatter, Quotes, ...).

const LABEL_CHARS: fn(char) -> bool = |c| c.is_ascii_alphanumeric() || c == '_';

/// `/\A[A-Za-z_](\w*[\w!?])?\z/`
fn is_label(value: &str) -> bool {
    let chars: Vec<char> = value.chars().collect();
    let Some(&first) = chars.first() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    if chars.len() == 1 {
        return true;
    }
    let last = chars[chars.len() - 1];
    chars[1..chars.len() - 1].iter().all(|&c| LABEL_CHARS(c))
        && (LABEL_CHARS(last) || last == '!' || last == '?')
}

impl Formatter<'_> {
    // ------------------------------------------------------------------
    // ArrayLiteral
    // ------------------------------------------------------------------

    fn format_array(&mut self, lbracket: Option<NodeId>, contents: Option<NodeId>) {
        let lbracket_plain = lbracket.is_some_and(|l| {
            matches!(self.k(l), Kind::LBracket { .. }) && self.comments(l).is_empty()
        });
        if lbracket_plain
            && let Some(contents) = contents
            && self.comments(contents).is_empty()
            && let Kind::Args { parts } = self.k(contents)
            && parts.len() > 1
        {
            let parts = parts.clone();
            if self.qwords(&parts) {
                self.text("%w[");
                self.group(|q| {
                    q.indent(|q| {
                        q.breakable_empty();
                        for (index, &part) in parts.iter().enumerate() {
                            if index > 0 {
                                q.breakable_space();
                            }
                            match q.k(part) {
                                Kind::StringLiteral { parts, .. } => {
                                    let first = parts[0];
                                    q.format(first);
                                }
                                Kind::CHAR { value } => {
                                    let rest: String = value.chars().skip(1).collect();
                                    q.text_owned(rest);
                                }
                                _ => {}
                            }
                        }
                    });
                    q.breakable_empty();
                });
                self.text("]");
                return;
            }
            if parts.iter().all(|&p| {
                matches!(self.k(p), Kind::SymbolLiteral { .. }) && self.comments(p).is_empty()
            }) {
                self.text("%i[");
                self.group(|q| {
                    q.indent(|q| {
                        q.breakable_empty();
                        for (index, &part) in parts.iter().enumerate() {
                            if index > 0 {
                                q.breakable_space();
                            }
                            if let Kind::SymbolLiteral { value } = q.k(part) {
                                let value = *value;
                                q.format(value);
                            }
                        }
                    });
                    q.breakable_empty();
                });
                self.text("]");
                return;
            }
        }

        // empty_with_comments?
        if contents.is_none()
            && let Some(lbracket) = lbracket
            && !self.comments(lbracket).is_empty()
            && !self.comments(lbracket).iter().any(|&c| self.is_comment_inline(c))
        {
            let comments = self.comments(lbracket).to_vec();
            self.group(|q| {
                q.text("[");
                q.indent(|q| {
                    for &comment in &comments {
                        q.breakable_force();
                        q.format_comment(comment);
                    }
                });
                q.breakable_force();
                q.text("]");
            });
            return;
        }

        let trailing_comma = self.options.trailing_comma;
        self.group(|q| {
            if let Some(lbracket) = lbracket {
                q.format(lbracket);
            }
            if let Some(contents) = contents {
                q.indent(|q| {
                    q.breakable_empty();
                    q.format(contents);
                    if trailing_comma {
                        q.if_break_only(|q| q.text(","));
                    }
                });
            }
            q.breakable_empty();
            q.text("]");
        });
    }

    fn qwords(&self, parts: &[NodeId]) -> bool {
        parts.iter().all(|&part| match self.k(part) {
            Kind::StringLiteral { parts, .. } => {
                self.comments(part).is_empty()
                    && parts.len() == 1
                    && match self.k(parts[0]) {
                        Kind::TStringContent { value } => !value.chars().any(|c| {
                            matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}' | '\u{b}' | '[' | ']' | '\\')
                        }),
                        _ => false,
                    }
            }
            Kind::CHAR { value } => !value.chars().any(|c| matches!(c, '[' | ']' | '\\')),
            _ => false,
        })
    }

    // ------------------------------------------------------------------
    // Hashes
    // ------------------------------------------------------------------

    fn format_assoc_contents(&mut self, key: NodeId, value: Option<NodeId>) {
        let formatter = match self.parent_node() {
            Some(parent) => match self.k(parent) {
                Kind::HashLiteral { .. } => self.hash_key_formatter(parent),
                Kind::BareAssocHash { .. } => self.bare_hash_key_formatter(parent),
                _ => KeyFormatter::Identity,
            },
            None => KeyFormatter::Identity,
        };
        self.format_key_with(formatter, key);
        let Some(value) = value else {
            return;
        };
        if self.comments(key).is_empty() && self.skip_indent(value) {
            self.text(" ");
            self.format(value);
        } else {
            self.indent(|q| {
                q.breakable_space();
                q.format(value);
            });
        }
    }

    fn format_key_with(&mut self, formatter: KeyFormatter, key: NodeId) {
        match formatter {
            KeyFormatter::Labels => match self.k(key) {
                Kind::Label { .. } => {
                    self.format(key);
                }
                Kind::SymbolLiteral { value } => {
                    let value = *value;
                    self.format(value);
                    self.text(":");
                }
                Kind::DynaSymbol { parts, .. } => {
                    if parts.len() == 1
                        && let Kind::TStringContent { value } = self.k(parts[0])
                        && is_label(value)
                    {
                        let part = parts[0];
                        self.format(part);
                        self.text(":");
                    } else {
                        self.format(key);
                        self.text(":");
                    }
                }
                _ => {}
            },
            KeyFormatter::Rockets => {
                match self.k(key) {
                    Kind::Label { value } => {
                        let value = value.strip_suffix(':').unwrap_or(value);
                        self.text_owned(format!(":{value}"));
                    }
                    Kind::DynaSymbol { .. } => {
                        self.text(":");
                        self.format(key);
                    }
                    _ => {
                        self.format(key);
                    }
                }
                self.text(" =>");
            }
            KeyFormatter::Identity => {
                if matches!(self.k(key), Kind::Label { .. }) {
                    self.format(key);
                } else {
                    self.format(key);
                    self.text(" =>");
                }
            }
        }
    }

    fn hash_key_formatter(&mut self, container: NodeId) -> KeyFormatter {
        if let Some(&formatter) = self.key_formatters.get(&container) {
            return formatter;
        }
        let assocs = match self.k(container) {
            Kind::HashLiteral { assocs, .. } | Kind::BareAssocHash { assocs } => assocs.clone(),
            _ => vec![],
        };
        let formatter = self.key_formatter_for(&assocs);
        self.key_formatters.insert(container, formatter);
        formatter
    }

    fn bare_hash_key_formatter(&mut self, container: NodeId) -> KeyFormatter {
        if let Some(&formatter) = self.key_formatters.get(&container) {
            return formatter;
        }
        // `q.parents.take(3).last`, where the current node is the Assoc.
        let parents = self.parents();
        let ancestor = parents.get(2).or_else(|| parents.last()).copied();
        let formatter = match self.entry_kind(ancestor) {
            Some(Kind::Break { .. } | Kind::Next { .. } | Kind::ReturnNode { .. }) => {
                KeyFormatter::Identity
            }
            _ => {
                let assocs = match self.k(container) {
                    Kind::BareAssocHash { assocs } => assocs.clone(),
                    _ => vec![],
                };
                self.key_formatter_for(&assocs)
            }
        };
        self.key_formatters.insert(container, formatter);
        formatter
    }

    /// `HashKeyFormatter.for`
    fn key_formatter_for(&self, assocs: &[NodeId]) -> KeyFormatter {
        let omitted_value = |rest: &[NodeId]| {
            rest.iter().any(|&a| match self.k(a) {
                Kind::Assoc { value, .. } => value.is_none(),
                _ => false,
            })
        };
        for (index, &assoc) in assocs.iter().enumerate() {
            let Kind::Assoc { key, value } = self.k(assoc) else {
                continue;
            };
            if value.is_none() {
                return KeyFormatter::Identity;
            }
            match self.k(*key) {
                Kind::Label { .. } | Kind::DynaSymbol { .. } => {}
                Kind::SymbolLiteral { value } => {
                    let value = self.value(*value);
                    let first_ok = value
                        .chars()
                        .next()
                        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic());
                    if !first_ok || value.ends_with('=') {
                        return if omitted_value(&assocs[index + 1..]) {
                            KeyFormatter::Identity
                        } else {
                            KeyFormatter::Rockets
                        };
                    }
                }
                _ => {
                    return if omitted_value(&assocs[index + 1..]) {
                        KeyFormatter::Identity
                    } else {
                        KeyFormatter::Rockets
                    };
                }
            }
        }
        KeyFormatter::Labels
    }

    fn format_hash_contents(&mut self, id: NodeId, lbrace: NodeId, assocs: &[NodeId]) {
        let lbrace_comments = self.comments(lbrace).to_vec();
        if assocs.is_empty()
            && !lbrace_comments.is_empty()
            && !lbrace_comments.iter().any(|&c| self.is_comment_inline(c))
        {
            self.group(|q| {
                q.text("{");
                q.indent(|q| {
                    for &comment in &lbrace_comments {
                        q.breakable_force();
                        q.format_comment(comment);
                    }
                });
                q.breakable_force();
                q.text("}");
            });
            return;
        }
        let _ = id;
        self.format(lbrace);
        if assocs.is_empty() {
            self.breakable_empty();
        } else {
            let trailing_comma = self.options.trailing_comma;
            self.indent(|q| {
                q.breakable_space();
                q.seplist_nodes(assocs);
                if trailing_comma {
                    q.if_break_only(|q| q.text(","));
                }
            });
            self.breakable_space();
        }
        self.text("}");
    }

    // ------------------------------------------------------------------
    // Binary
    // ------------------------------------------------------------------

    fn format_binary(&mut self, left: NodeId, operator: &'static str, right: NodeId) {
        let power = operator == "**";
        let left_is_shovel = matches!(self.k(left), Kind::Binary { operator, .. } if operator == "<<");
        self.group(|q| {
            q.group(|q| {
                q.format(left);
            });
            if !power {
                q.text(" ");
            }
            if operator != "<<" || left_is_shovel {
                q.group(|q| {
                    q.text(operator);
                    q.indent(|q| {
                        if power {
                            q.breakable_empty();
                        } else {
                            q.breakable_space();
                        }
                        q.format(right);
                    });
                });
            } else {
                q.text("<< ");
                q.format(right);
            }
        });
    }

    // ------------------------------------------------------------------
    // FlowControlFormatter
    // ------------------------------------------------------------------

    fn format_flow_control(&mut self, keyword: &'static str, arguments: Option<NodeId>) {
        let Some(arguments) = arguments else {
            self.text(keyword);
            return;
        };
        let parts = match self.k(arguments) {
            Kind::Args { parts } => parts.clone(),
            _ => vec![arguments],
        };
        self.group(|q| {
            q.text(keyword);
            let length = parts.len();
            if length == 0 {
                return;
            }
            if length >= 2 {
                q.flow_format_arguments(arguments, " [", "]");
                return;
            }
            let part = parts[0];
            match q.k(part) {
                Kind::Paren { contents, .. } => {
                    let body = contents.and_then(|c| match q.k(c) {
                        Kind::Statements { body } => Some(body.clone()),
                        _ => None,
                    });
                    match body {
                        Some(body) if body.len() == 1 => {
                            let statement = body[0];
                            if let Kind::ArrayLiteral { contents, .. } = q.k(statement) {
                                let long = contents.is_some_and(|c| {
                                    matches!(q.k(c), Kind::Args { parts } if parts.len() >= 2)
                                });
                                q.text(" ");
                                if long {
                                    q.flow_format_array_contents(statement);
                                } else {
                                    q.format(statement);
                                }
                            } else if q.flow_skip_parens(statement) {
                                q.text(" ");
                                q.format(statement);
                            } else {
                                q.format(part);
                            }
                        }
                        _ => {
                            q.format(part);
                        }
                    }
                }
                Kind::ArrayLiteral { contents, .. } => {
                    let long = contents
                        .is_some_and(|c| matches!(q.k(c), Kind::Args { parts } if parts.len() >= 2));
                    q.text(" ");
                    if long {
                        q.flow_format_array_contents(part);
                    } else {
                        q.format(part);
                    }
                }
                _ => q.flow_format_arguments(arguments, "(", ")"),
            }
        });
    }

    fn flow_format_array_contents(&mut self, array: NodeId) {
        let Kind::ArrayLiteral { contents, .. } = self.k(array) else {
            return;
        };
        let contents = *contents;
        self.if_break_only(|q| q.text("["));
        self.indent(|q| {
            q.breakable_empty();
            if let Some(contents) = contents {
                q.format(contents);
            }
        });
        self.breakable_empty();
        self.if_break_only(|q| q.text("]"));
    }

    fn flow_format_arguments(&mut self, arguments: NodeId, opening: &'static str, closing: &'static str) {
        self.if_break_only(|q| q.text(opening));
        self.indent(|q| {
            q.breakable_space();
            q.format(arguments);
        });
        self.breakable_empty();
        self.if_break_only(|q| q.text(closing));
    }

    fn flow_skip_parens(&self, node: NodeId) -> bool {
        match self.k(node) {
            Kind::FloatLiteral { .. }
            | Kind::Imaginary { .. }
            | Kind::Int { .. }
            | Kind::RationalLiteral { .. } => true,
            Kind::VarRef { value } => matches!(
                self.k(*value),
                Kind::Const { .. } | Kind::CVar { .. } | Kind::GVar { .. } | Kind::IVar { .. } | Kind::Kw { .. }
            ),
            _ => false,
        }
    }

    // ------------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------------

    /// `ArgParen#trailing_comma?`
    fn arg_paren_trailing_comma(&self, arguments: NodeId) -> bool {
        let Kind::Args { parts } = self.k(arguments) else {
            return false;
        };
        if parts
            .last()
            .is_some_and(|&p| matches!(self.k(p), Kind::ArgBlock { .. }))
        {
            false
        } else {
            !(parts.len() == 1
                && matches!(self.k(parts[0]), Kind::Command { .. } | Kind::CommandCall { .. }))
        }
    }

    /// `CallChainFormatter.chained?`
    fn chained(&self, entry: Option<Entry>) -> bool {
        match self.entry_kind(entry) {
            Some(Kind::CallNode { receiver, .. }) => receiver.is_some(),
            Some(Kind::MethodAddBlock { call, .. }) => {
                matches!(self.k(*call), Kind::CallNode { receiver: Some(_), .. })
            }
            _ => false,
        }
    }

    fn format_call(&mut self, id: NodeId) {
        let Kind::CallNode {
            receiver,
            message,
            arguments,
            ..
        } = *self.k(id)
        else {
            return;
        };
        if let Some(receiver) = receiver {
            if self.chained(Some(Entry::Node(receiver))) && !self.chained(self.parent()) {
                self.group(|q| {
                    q.if_break(|q| q.format_call_chain(id), |q| q.format_call_contents(id));
                });
            } else {
                self.format_call_contents(id);
            }
        } else {
            if let Some(message) = message {
                self.format(message);
            }
            if let Some(arguments) = arguments {
                self.format(arguments);
            }
        }
    }

    /// `CallNode#format_arguments`
    fn format_call_arguments(&mut self, id: NodeId) {
        let Kind::CallNode { arguments, .. } = *self.k(id) else {
            return;
        };
        let Some(arguments) = arguments else {
            return;
        };
        match self.k(arguments) {
            Kind::ArgParen { .. } => {
                self.format(arguments);
            }
            Kind::Args { .. } => {
                self.text(" ");
                self.format(arguments);
            }
            _ => {}
        }
    }

    /// `CallNode#format_contents`
    fn format_call_contents(&mut self, id: NodeId) {
        let Kind::CallNode {
            receiver,
            operator,
            message,
            ..
        } = *self.k(id)
        else {
            return;
        };
        let operator_comments = !self
            .entry_comments(Entry::Helper(Helper::CallOperator(operator)))
            .is_empty();
        let receiver_comments = receiver.is_some_and(|r| !self.comments(r).is_empty());
        self.group(|q| {
            if let Some(receiver) = receiver {
                q.format(receiver);
            }
            if operator_comments {
                q.format_helper(Helper::CallOperator(operator), true);
            }
            q.group(|q| {
                q.indent(|q| {
                    if receiver_comments || operator_comments {
                        q.breakable_force();
                    }
                    if !operator_comments {
                        q.format_helper(Helper::CallOperator(operator), false);
                    }
                    if let Some(message) = message {
                        q.format(message);
                    }
                });
                q.format_call_arguments(id);
            });
        });
    }

    fn format_method_add_block_contents(&mut self, id: NodeId) {
        let Kind::MethodAddBlock { call, block } = *self.k(id) else {
            return;
        };
        self.format(call);
        self.format(block);
    }

    /// `CallChainFormatter#format`
    fn format_call_chain(&mut self, node: NodeId) {
        let mut children = vec![node];
        let mut threshold = 3;
        loop {
            let child = *children.last().unwrap();
            match self.k(child) {
                Kind::CallNode { receiver, .. } => match receiver.map(|r| (r, self.k(r))) {
                    Some((r, Kind::CallNode { receiver: rr, .. })) => {
                        if rr.is_none() {
                            break;
                        }
                        children.push(r);
                    }
                    Some((r, Kind::MethodAddBlock { call, .. })) => {
                        if matches!(self.k(*call), Kind::CallNode { receiver: Some(_), .. }) {
                            children.push(r);
                        } else {
                            break;
                        }
                    }
                    _ => break,
                },
                Kind::MethodAddBlock { call, .. } => {
                    if matches!(self.k(*call), Kind::CallNode { receiver: Some(_), .. }) {
                        children.push(*call);
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        // Sorbet `sig` blocks lower the threshold.
        let parents: Vec<Entry> = self.parents().into_iter().take(4).collect();
        if let Some(&parent) = parents.get(2) {
            let mut parent = parent;
            if let Some(Kind::BlockNode { opening, .. }) = self.entry_kind(Some(parent))
                && matches!(self.k(*opening), Kind::Kw { .. })
                && let Some(&p3) = parents.get(3)
            {
                parent = p3;
            }
            if let Some(Kind::MethodAddBlock { call, .. }) = self.entry_kind(Some(parent))
                && let Kind::CallNode {
                    message: Some(message),
                    ..
                } = self.k(*call)
                && self.value(*message) == "sig"
            {
                threshold = 2;
            }
        }

        if children.len() >= threshold {
            self.group(|q| {
                q.if_break(
                    |q| q.format_chain(node, children.clone()),
                    |q| q.format_chain_node_contents(node),
                );
            });
        } else {
            self.format_chain_node_contents(node);
        }
    }

    fn format_chain_node_contents(&mut self, node: NodeId) {
        match self.k(node) {
            Kind::CallNode { .. } => self.format_call_contents(node),
            Kind::MethodAddBlock { .. } => self.format_method_add_block_contents(node),
            _ => {}
        }
    }

    fn call_receiver(&self, node: NodeId) -> Option<NodeId> {
        match self.k(node) {
            Kind::CallNode { receiver, .. } => *receiver,
            _ => None,
        }
    }

    /// `CallChainFormatter#format_chain`
    fn format_chain(&mut self, node: NodeId, mut children: Vec<NodeId>) {
        let empty_except_last = children.iter().skip(1).all(|&child| {
            matches!(self.k(child), Kind::CallNode { arguments: None, .. })
        });

        self.stack.extend(children.iter().map(|&c| Entry::Node(c)));
        let last = *children.last().unwrap();
        if let Some(receiver) = self.call_receiver(last) {
            self.format(receiver);
        }

        self.group(|q| {
            let last = *children.last().unwrap();
            if q.attach_directly(last) {
                let child = children.pop().unwrap();
                q.format_chain_child(child, false, false, false);
                q.stack.pop();
            }

            q.indent(|q| {
                let mut skip_operator = false;
                while let Some(child) = children.pop() {
                    if let Kind::CallNode { receiver, message, .. } = *q.k(child) {
                        let where_not = receiver.is_some_and(|r| {
                            matches!(
                                q.k(r),
                                Kind::CallNode { message: Some(rm), .. } if q.value(*rm) == "where"
                            )
                        }) && message.is_some_and(|m| q.value(m) == "not");
                        if !where_not {
                            q.breakable_empty();
                        }
                    }

                    let last_in_chain = children.is_empty();
                    q.format_chain_child(
                        child,
                        last_in_chain,
                        skip_operator,
                        empty_except_last && last_in_chain,
                    );

                    let next = children.last().copied();
                    skip_operator = false;
                    if let Some(next) = next
                        && let Kind::CallNode {
                            operator,
                            message: Some(message),
                            ..
                        } = *q.k(next)
                        && let CallOperator::Node(op) = operator
                        && (!q.comments(message).is_empty() || !q.comments(op).is_empty())
                    {
                        q.format_helper(Helper::CallOperator(operator), true);
                        skip_operator = true;
                    }

                    q.stack.pop();
                }
            });
        });

        if empty_except_last {
            match self.k(node) {
                Kind::CallNode { .. } => self.format_call_arguments(node),
                Kind::MethodAddBlock { block, .. } => {
                    let block = *block;
                    self.format(block);
                }
                _ => {}
            }
        }
    }

    fn attach_directly(&self, node: NodeId) -> bool {
        match self.call_receiver(node) {
            Some(receiver) => matches!(
                self.k(receiver),
                Kind::ArrayLiteral { .. }
                    | Kind::HashLiteral { .. }
                    | Kind::Heredoc { .. }
                    | Kind::IfNode { .. }
                    | Kind::UnlessNode { .. }
                    | Kind::XStringLiteral { .. }
            ),
            None => false,
        }
    }

    fn format_chain_child(
        &mut self,
        child: NodeId,
        skip_comments: bool,
        skip_operator: bool,
        skip_attached: bool,
    ) {
        match *self.k(child) {
            Kind::CallNode {
                operator, message, ..
            } => {
                self.group(|q| {
                    if !skip_operator && operator != CallOperator::Nil {
                        q.format_helper(Helper::CallOperator(operator), true);
                    }
                    if let Some(message) = message {
                        q.format(message);
                    }
                    if !skip_attached {
                        q.format_call_arguments(child);
                    }
                });
            }
            Kind::MethodAddBlock { block, .. } => {
                if !skip_attached {
                    self.format(block);
                }
            }
            _ => {}
        }

        let comments = self.comments(child).to_vec();
        if !comments.is_empty() && !skip_comments {
            for comment in comments {
                if self.is_comment_inline(comment) {
                    self.text(" ");
                } else {
                    self.breakable_space();
                }
                self.format_comment(comment);
            }
            self.break_parent();
        }
    }

    /// `Command#align`
    fn command_align(&mut self, message: NodeId, node: NodeId, arguments_of_self: NodeId) {
        let _ = arguments_of_self;
        let message_width = self.value(message).chars().count() as i64 + 1;
        let arguments = match self.k(node) {
            Kind::Command { arguments, .. } => *arguments,
            _ => return,
        };
        if let Kind::Args { parts } = self.k(arguments)
            && parts.len() == 1
        {
            let part = parts[0];
            match self.k(part) {
                Kind::DefNode { .. } => {
                    self.text(" ");
                    self.format(arguments_of_root(self, node));
                }
                Kind::IfOp { .. } => {
                    self.if_flat(|q| q.text(" "));
                    self.format(arguments_of_root(self, node));
                }
                Kind::Command { .. } => {
                    // align(q, part): recurse with the nested command's
                    // arguments but this command's message width.
                    self.command_align_nested(message_width, part, node);
                }
                _ => {
                    self.text(" ");
                    let args = arguments_of_root(self, node);
                    self.nest(message_width, |q| {
                        q.format(args);
                    });
                }
            }
        } else {
            self.text(" ");
            let args = arguments_of_root(self, node);
            self.nest(message_width, |q| {
                q.format(args);
            });
        }
    }

    /// The recursive branch of `Command#align`: decides alignment from the
    /// nested command's arguments, but always yields the root's arguments.
    fn command_align_nested(&mut self, message_width: i64, node: NodeId, root: NodeId) {
        let arguments = match self.k(node) {
            Kind::Command { arguments, .. } => *arguments,
            _ => return,
        };
        let root_args = arguments_of_root(self, root);
        if let Kind::Args { parts } = self.k(arguments)
            && parts.len() == 1
        {
            let part = parts[0];
            match self.k(part) {
                Kind::DefNode { .. } => {
                    self.text(" ");
                    self.format(root_args);
                }
                Kind::IfOp { .. } => {
                    self.if_flat(|q| q.text(" "));
                    self.format(root_args);
                }
                Kind::Command { .. } => self.command_align_nested(message_width, part, root),
                _ => {
                    self.text(" ");
                    self.nest(message_width, |q| {
                        q.format(root_args);
                    });
                }
            }
        } else {
            self.text(" ");
            self.nest(message_width, |q| {
                q.format(root_args);
            });
        }
    }

    fn format_command_call(&mut self, id: NodeId) {
        let Kind::CommandCall {
            receiver,
            operator,
            message,
            arguments,
            block,
        } = *self.k(id)
        else {
            return;
        };
        let leading_message_comments = message.is_some_and(|m| {
            self.comments(m)
                .iter()
                .any(|&c| self.is_comment_leading(c))
        });
        self.group(|q| {
            let doc = q.nest(0, |q| {
                q.format(receiver);
                if leading_message_comments {
                    q.format_helper(Helper::CallOperator(operator), false);
                    q.indent(|q| {
                        q.breakable_empty();
                        if let Some(message) = message {
                            q.format(message);
                        }
                    });
                } else {
                    q.format_helper(Helper::CallOperator(operator), false);
                    if let Some(message) = message {
                        q.format(message);
                    }
                }
            });

            if let Some(arguments) = arguments {
                let parts = match q.k(arguments) {
                    Kind::Args { parts } => parts.clone(),
                    _ => vec![],
                };
                if parts.len() == 1 && matches!(q.k(parts[0]), Kind::IfOp { .. }) {
                    q.if_flat(|q| q.text(" "));
                    q.format(arguments);
                } else {
                    q.text(" ");
                    let rspec = message.is_some_and(|m| {
                        matches!(q.value(m), "to" | "not_to" | "to_not")
                    });
                    let alignment = if rspec {
                        0
                    } else {
                        let width = q.pp.last_position(doc) + 1;
                        if width > q.pp.maxwidth / 2 { 0 } else { width }
                    };
                    q.nest(alignment as i64, |q| {
                        q.format(arguments);
                    });
                }
            }
        });
        if let Some(block) = block {
            self.format(block);
        }
    }

    // ------------------------------------------------------------------
    // Definitions
    // ------------------------------------------------------------------

    fn format_def(&mut self, id: NodeId) {
        let Kind::DefNode {
            target,
            operator,
            name,
            params,
            bodystmt,
        } = *self.k(id)
        else {
            return;
        };
        let endless = !matches!(self.k(bodystmt), Kind::BodyStmt { .. });
        let name_has_comments = !self.comments(name).is_empty();
        self.group(|q| {
            q.group(|q| {
                q.text("def");
                if target.is_some() || !name_has_comments {
                    q.text(" ");
                }
                if let Some(target) = target {
                    q.format(target);
                    q.format_helper(Helper::CallOperator(operator), false);
                }
                q.format(name);
                if let Some(params) = params {
                    match q.k(params) {
                        Kind::Paren { .. } => {
                            q.format(params);
                        }
                        Kind::Params { .. } => {
                            if !q.params_empty(params) || !q.comments(params).is_empty() {
                                q.format(params);
                            }
                        }
                        _ => {}
                    }
                }
            });

            if endless {
                q.text(" =");
                q.group(|q| {
                    q.indent(|q| {
                        q.breakable_space();
                        q.format(bodystmt);
                    });
                });
            } else {
                if !q.bodystmt_empty(bodystmt) {
                    q.indent(|q| {
                        q.breakable_force();
                        q.format(bodystmt);
                    });
                }
                q.breakable_force();
                q.text("end");
            }
        });
    }

    fn format_params(&mut self, id: NodeId) -> DocId {
        let Kind::Params {
            requireds,
            optionals,
            rest,
            posts,
            keywords,
            keyword_rest,
            block,
        } = self.k(id)
        else {
            unreachable!()
        };
        let mut parts: Vec<Entry> = requireds.iter().map(|&r| Entry::Node(r)).collect();
        parts.extend(
            optionals
                .iter()
                .map(|&(name, value)| Entry::Helper(Helper::ParamsOptional(name, value))),
        );
        let excessed = rest.filter(|&r| matches!(self.k(r), Kind::ExcessedComma { .. }));
        if let Some(rest) = rest
            && excessed.is_none()
        {
            parts.push(Entry::Node(*rest));
        }
        parts.extend(posts.iter().map(|&p| Entry::Node(p)));
        parts.extend(
            keywords
                .iter()
                .map(|&(name, value)| Entry::Helper(Helper::ParamsKeyword(name, value))),
        );
        if *keyword_rest != NilOr::None {
            parts.push(Entry::Helper(Helper::ParamsKeywordRest(*keyword_rest)));
        }
        if let Some(block) = block {
            parts.push(Entry::Node(*block));
        }

        let contents = |q: &mut Self| {
            for (index, &part) in parts.iter().enumerate() {
                if index > 0 {
                    q.comma_breakable();
                }
                q.format_entry(part, true);
            }
            if let Some(excessed) = excessed {
                q.format(excessed);
            }
        };

        if parts.is_empty() {
            return self.nest(0, contents);
        }
        if matches!(self.parent_kind(), Some(Kind::DefNode { .. })) {
            self.nest(0, |q| {
                q.text("(");
                q.group(|q| {
                    q.indent(|q| {
                        q.breakable_empty();
                        contents(q);
                    });
                    q.breakable_empty();
                });
                q.text(")");
            })
        } else {
            self.nest(0, contents)
        }
    }

    // ------------------------------------------------------------------
    // Blocks
    // ------------------------------------------------------------------

    fn format_block(&mut self, id: NodeId) {
        let Kind::BlockNode {
            opening,
            block_var,
            bodystmt,
        } = *self.k(id)
        else {
            return;
        };
        let keywords = matches!(self.k(opening), Kind::Kw { .. });
        let value = |s: &'static str| BlockText::Static(s);
        let (break_opening, break_closing, flat_opening, flat_closing) =
            if self.block_unchangeable_bounds() {
                let close = if keywords { "end" } else { "}" };
                (
                    BlockText::Value(opening),
                    close,
                    BlockText::Value(opening),
                    close,
                )
            } else if self.block_forced_do_end_bounds() {
                (value("do"), "end", value("do"), "end")
            } else if self.block_forced_brace_bounds() {
                (value("{"), "}", value("{"), "}")
            } else {
                (value("do"), "end", value("{"), "}")
            };

        match self.parent_kind() {
            None | Some(Kind::Command { .. } | Kind::CommandCall { .. }) => {
                self.break_parent();
                self.format_block_break(opening, block_var, bodystmt, break_opening, break_closing);
                return;
            }
            _ => {}
        }

        self.group(|q| {
            q.if_break(
                |q| q.format_block_break(opening, block_var, bodystmt, break_opening, break_closing),
                |q| q.format_block_flat(opening, block_var, bodystmt, flat_opening, flat_closing),
            );
        });
    }

    fn block_unchangeable_bounds(&self) -> bool {
        for parent in self.parents() {
            match self.entry_kind(Some(parent)) {
                Some(Kind::Statements { .. } | Kind::ArgParen { .. }) => return false,
                Some(Kind::Command { .. } | Kind::CommandCall { .. }) => return true,
                _ => {}
            }
        }
        false
    }

    fn block_forced_do_end_bounds(&self) -> bool {
        let call = match self.parent_kind() {
            Some(Kind::MethodAddBlock { call, .. }) => *call,
            _ => return false,
        };
        matches!(
            self.k(call),
            Kind::Break { .. } | Kind::Next { .. } | Kind::ReturnNode { .. } | Kind::Super { .. }
        )
    }

    fn block_forced_brace_bounds(&self) -> bool {
        let mut previous: Option<Entry> = None;
        for parent in self.parents() {
            match self.entry_kind(Some(parent)) {
                Some(Kind::Paren { .. } | Kind::Statements { .. }) => return false,
                Some(
                    Kind::IfNode { predicate, .. }
                    | Kind::UnlessNode { predicate, .. }
                    | Kind::WhileNode { predicate, .. }
                    | Kind::UntilNode { predicate, .. }
                    | Kind::IfOp { predicate, .. },
                ) => {
                    if let Some(Entry::Node(prev)) = previous
                        && prev == *predicate
                    {
                        return true;
                    }
                }
                _ => {}
            }
            previous = Some(parent);
        }
        false
    }

    fn block_text_is_do(&self, text: BlockText) -> bool {
        match text {
            BlockText::Static(s) => s == "do",
            BlockText::Value(node) => self.value(node) == "do",
        }
    }

    fn format_block_break(
        &mut self,
        opening: NodeId,
        block_var: Option<NodeId>,
        bodystmt: NodeId,
        break_opening: BlockText,
        break_closing: &'static str,
    ) {
        self.text(" ");
        self.format_helper(
            Helper::BlockOpen {
                text: break_opening,
                node: opening,
            },
            false,
        );
        if let Some(block_var) = block_var {
            self.text(" ");
            self.format(block_var);
        }
        if !self.bodystmt_empty(bodystmt) {
            self.indent(|q| {
                q.breakable_space();
                q.format(bodystmt);
            });
        }
        self.breakable_space();
        self.text(break_closing);
    }

    fn format_block_flat(
        &mut self,
        opening: NodeId,
        block_var: Option<NodeId>,
        bodystmt: NodeId,
        flat_opening: BlockText,
        flat_closing: &'static str,
    ) {
        self.text(" ");
        self.format_helper(
            Helper::BlockOpen {
                text: flat_opening,
                node: opening,
            },
            false,
        );
        if let Some(block_var) = block_var {
            self.breakable_space();
            self.format(block_var);
            self.breakable_space();
        }
        if self.bodystmt_empty(bodystmt) {
            if self.block_text_is_do(flat_opening) {
                self.text(" ");
            }
        } else {
            if block_var.is_none() {
                self.breakable_space();
            }
            self.format(bodystmt);
            self.breakable_space();
        }
        self.text(flat_closing);
    }

    fn format_lambda(&mut self, id: NodeId) {
        let Kind::Lambda { params, statements } = *self.k(id) else {
            return;
        };
        let statements_empty = self.bodystmt_empty(statements);
        self.text("->");
        self.group(|q| {
            match q.k(params) {
                Kind::Paren { contents, .. } => {
                    let empty = contents.is_none_or(|c| q.lambda_var_empty(c));
                    if !empty {
                        q.format(params);
                    }
                }
                _ => {
                    let empty = q.lambda_var_empty(params);
                    if empty && !q.comments(params).is_empty() {
                        q.format(params);
                    } else if !empty {
                        q.group(|q| {
                            q.text("(");
                            q.format(params);
                            q.text(")");
                        });
                    }
                }
            }
            q.text(" ");
            q.if_break(
                |q| {
                    q.text("do");
                    if !statements_empty {
                        q.indent(|q| {
                            q.breakable_space();
                            q.format(statements);
                        });
                    }
                    q.breakable_space();
                    q.text("end");
                },
                |q| {
                    q.text("{");
                    if !statements_empty {
                        q.text(" ");
                        q.format(statements);
                        q.text(" ");
                    }
                    q.text("}");
                },
            );
        });
    }

    fn lambda_var_empty(&self, id: NodeId) -> bool {
        match self.k(id) {
            Kind::LambdaVar { params, locals } => self.params_empty(*params) && locals.is_empty(),
            Kind::Params { .. } => self.params_empty(id),
            _ => false,
        }
    }

    // ------------------------------------------------------------------
    // Strings
    // ------------------------------------------------------------------

    /// `Quotes.locked?`
    fn quotes_locked(&self, parts: &[NodeId], quote: &str) -> bool {
        parts.iter().any(|&part| match self.k(part) {
            Kind::TStringContent { value } => {
                value.contains('\\')
                    || value.contains("#@")
                    || value.contains("#$")
                    || value.contains("#{")
                    || value.contains(quote)
            }
            _ => true,
        })
    }

    fn format_string_parts(&mut self, parts: &[NodeId], closing_quote: &str) {
        for &part in parts {
            if let Kind::TStringContent { value } = self.k(part) {
                let value = quotes_normalize(value, closing_quote);
                let mut first = true;
                for line in each_line_chomp(&value) {
                    if first {
                        first = false;
                    } else {
                        self.breakable_return();
                    }
                    self.text_owned(line.to_string());
                }
                if value.ends_with('\n') {
                    self.breakable_return();
                }
            } else {
                self.format(part);
            }
        }
    }

    fn format_string_literal(&mut self, id: NodeId) {
        let Kind::StringLiteral { parts, quote } = self.k(id) else {
            return;
        };
        let q_quote = self.options.quote;
        if parts.is_empty() {
            self.text_owned(format!("{q_quote}{q_quote}"));
            return;
        }
        let parts = parts.clone();
        let (opening, closing): (String, String) = if !self.quotes_locked(&parts, q_quote) {
            (q_quote.to_string(), q_quote.to_string())
        } else if let Some(quote) = quote.as_deref().filter(|q| q.starts_with('%')) {
            // quote[/%[qQ]?(.)/, 1]
            let rest = &quote[1..];
            let rest = rest
                .strip_prefix('q')
                .or_else(|| rest.strip_prefix('Q'))
                .unwrap_or(rest);
            let delimiter = rest.chars().next().unwrap_or('"');
            (quote.to_string(), quotes_matching(delimiter).to_string())
        } else {
            let quote = quote.clone().unwrap_or_default();
            (quote.clone(), quote)
        };
        let (opening_frozen, closing_frozen) = (
            !self.quotes_locked(&parts, q_quote),
            !self.quotes_locked(&parts, q_quote),
        );
        if opening_frozen {
            self.text(q_quote);
        } else {
            self.text_owned(opening);
        }
        self.group(|q| q.format_string_parts(&parts, &closing));
        if closing_frozen {
            self.text(q_quote);
        } else {
            self.text_owned(closing);
        }
    }

    fn format_dyna_symbol(&mut self, id: NodeId) {
        let Kind::DynaSymbol { parts, quote } = self.k(id) else {
            return;
        };
        let parts = parts.clone();
        let quote = quote.clone().unwrap_or_default();
        let q_quote = self.options.quote;
        let hash_key = matches!(self.parent_kind(), Some(Kind::Assoc { key, .. }) if *key == id);
        let colon = if hash_key { "" } else { ":" };

        let (opening, closing): (String, String) = if quote.starts_with("%s") {
            let delimiter = quote.chars().nth(2).unwrap_or('[');
            let matching = quotes_matching(delimiter);
            let matched = parts.iter().any(|&p| match self.k(p) {
                Kind::TStringContent { value } => value
                    .chars()
                    .any(|c| c == '\n' || c == matching || c == '\'' || c == '"'),
                _ => false,
            });
            if matched {
                (quote.clone(), matching.to_string())
            } else if self.quotes_locked(&parts, q_quote) {
                (format!("{colon}'"), "'".to_string())
            } else {
                (format!("{colon}{q_quote}"), q_quote.to_string())
            }
        } else if self.quotes_locked(&parts, q_quote) {
            if let Some(rest) = quote.strip_prefix(':') {
                (
                    if hash_key { rest.to_string() } else { quote.clone() },
                    rest.to_string(),
                )
            } else {
                (
                    if hash_key {
                        quote.clone()
                    } else {
                        format!(":{quote}")
                    },
                    quote.clone(),
                )
            }
        } else {
            (
                if hash_key {
                    q_quote.to_string()
                } else {
                    format!(":{q_quote}")
                },
                q_quote.to_string(),
            )
        };

        self.text_owned(opening);
        self.group(|q| q.format_string_parts(&parts, &closing));
        self.text_owned(closing);
    }

    fn format_regexp(&mut self, id: NodeId) {
        let Kind::RegexpLiteral {
            beginning,
            ending,
            parts,
        } = self.k(id)
        else {
            return;
        };
        let (beginning, ending, parts) = (beginning.clone(), ending.clone(), parts.clone());
        let includes = |q: &Self, f: &dyn Fn(&str) -> bool| {
            parts.iter().any(|&p| match q.k(p) {
                Kind::TStringContent { value } => f(value),
                _ => false,
            })
        };
        let braces = self.regexp_ambiguous(&parts) || includes(self, &|v| v.contains('/'));
        let options: String = ending.chars().skip(1).collect();

        if braces && includes(self, &|v| v.contains('{') || v.contains('}')) {
            self.group(|q| {
                q.text_owned(beginning.clone());
                for &part in &parts {
                    q.format(part);
                }
                q.text_owned(ending.clone());
            });
        } else if braces {
            self.group(|q| {
                q.text("%r{");
                if beginning == "/" {
                    for &part in &parts {
                        if let Kind::TStringContent { value } = q.k(part) {
                            q.text_owned(value.replace("\\/", "/"));
                        } else {
                            q.format(part);
                        }
                    }
                } else {
                    for &part in &parts {
                        q.format(part);
                    }
                }
                q.text("}");
                q.text_owned(options.clone());
            });
        } else {
            self.group(|q| {
                q.text("/");
                for &part in &parts {
                    q.format(part);
                }
                q.text("/");
                q.text_owned(options.clone());
            });
        }
    }

    fn regexp_ambiguous(&self, parts: &[NodeId]) -> bool {
        let Some(&part) = parts.first() else {
            return false;
        };
        match self.k(part) {
            Kind::TStringContent { value } if value.starts_with(' ') || value.starts_with('=') => self
                .parents()
                .iter()
                .any(|&p| matches!(self.entry_kind(Some(p)), Some(Kind::Command { .. } | Kind::CommandCall { .. }))),
            _ => false,
        }
    }

    // ------------------------------------------------------------------
    // Patterns
    // ------------------------------------------------------------------

    fn format_hshptn(&mut self, id: NodeId) {
        let Kind::HshPtn {
            constant,
            keywords,
            keyword_rest,
        } = self.k(id)
        else {
            return;
        };
        let (constant, keywords, keyword_rest) = (*constant, keywords.clone(), *keyword_rest);
        let mut parts: Vec<Helper> = keywords
            .iter()
            .map(|&(key, value)| Helper::HshPtnKeyword(key, value))
            .collect();
        if let Some(rest) = keyword_rest {
            parts.push(Helper::HshPtnKeywordRest(rest));
        }
        let nested = matches!(
            self.parent_kind(),
            Some(Kind::AryPtn { .. } | Kind::Binary { .. } | Kind::FndPtn { .. } | Kind::HshPtn { .. } | Kind::RAssign { .. })
        );
        let rest_is_bare = keyword_rest
            .is_some_and(|r| matches!(self.k(r), Kind::VarField { value: NilOr::None }));
        let contents = move |q: &mut Self| {
            q.group(|q| {
                for (index, &part) in parts.iter().enumerate() {
                    if index > 0 {
                        q.comma_breakable();
                    }
                    q.format_helper(part, false);
                }
            });
            if constant.is_none() && rest_is_bare && !nested {
                q.text(" then");
            }
        };

        if let Some(constant) = constant {
            self.group(|q| {
                q.format(constant);
                q.text("[");
                q.indent(|q| {
                    q.breakable_empty();
                    contents(q);
                });
                q.breakable_empty();
                q.text("]");
            });
            return;
        }
        let count = keywords.len() + usize::from(keyword_rest.is_some());
        if count == 0 {
            self.text("{}");
            return;
        }
        if !nested && count == 1 {
            contents(self);
            return;
        }
        let old_ruby = self.options.target_ruby_version < (2, 7, 3);
        self.group(|q| {
            q.text("{");
            q.indent(|q| {
                q.breakable_space();
                contents(q);
            });
            if old_ruby {
                q.text(" }");
            } else {
                q.breakable_space();
                q.text("}");
            }
        });
    }

    // ------------------------------------------------------------------
    // Conditionals
    // ------------------------------------------------------------------

    fn conditional_parts(&self, id: NodeId) -> (NodeId, NodeId, Option<NodeId>, &'static str) {
        match *self.k(id) {
            Kind::IfNode {
                predicate,
                statements,
                consequent,
            } => (predicate, statements, consequent, "if"),
            Kind::UnlessNode {
                predicate,
                statements,
                consequent,
            } => (predicate, statements, consequent, "unless"),
            _ => unreachable!(),
        }
    }

    fn is_modifier(&self, predicate: NodeId, statements: NodeId) -> bool {
        self.ast.loc(predicate).start_char > self.ast.loc(statements).start_char
    }

    fn statements_body(&self, id: NodeId) -> Vec<NodeId> {
        match self.k(id) {
            Kind::Statements { body } => body.clone(),
            _ => vec![],
        }
    }

    /// `Ternaryable.call`
    fn ternaryable(&self, node: NodeId) -> bool {
        if self.options.disable_auto_ternary {
            return false;
        }
        let (predicate, statements, consequent, _) = self.conditional_parts(node);
        if let Some(Kind::Paren {
            contents: Some(contents),
            ..
        }) = self.entry_kind(self.grandparent())
            && let Kind::Statements { body } = self.k(*contents)
            && body.len() == 1
            && body[0] == node
        {
            return false;
        }
        match self.k(predicate) {
            Kind::Assign { .. }
            | Kind::Binary { .. }
            | Kind::Command { .. }
            | Kind::CommandCall { .. }
            | Kind::MAssign { .. }
            | Kind::OpAssign { .. } => return false,
            Kind::Not { parentheses, .. } => {
                if !parentheses {
                    return false;
                }
            }
            _ => {}
        }
        let Some(consequent) = consequent else {
            return false;
        };
        let Kind::Else {
            statements: else_statements,
            ..
        } = self.k(consequent)
        else {
            return false;
        };
        let truthy = self.statements_body(statements);
        let falsy = self.statements_body(*else_statements);
        truthy.len() == 1
            && self.ternaryable_statement(truthy[0])
            && falsy.len() == 1
            && self.ternaryable_statement(falsy[0])
    }

    fn ternaryable_statement(&self, statement: NodeId) -> bool {
        match self.k(statement) {
            Kind::AliasNode { .. }
            | Kind::Assign { .. }
            | Kind::Break { .. }
            | Kind::Command { .. }
            | Kind::CommandCall { .. }
            | Kind::Defined { .. }
            | Kind::Heredoc { .. }
            | Kind::IfNode { .. }
            | Kind::IfOp { .. }
            | Kind::Lambda { .. }
            | Kind::MAssign { .. }
            | Kind::Next { .. }
            | Kind::OpAssign { .. }
            | Kind::RescueMod { .. }
            | Kind::ReturnNode { .. }
            | Kind::Super { .. }
            | Kind::Undef { .. }
            | Kind::UnlessNode { .. }
            | Kind::UntilNode { .. }
            | Kind::VoidStmt
            | Kind::WhileNode { .. }
            | Kind::YieldNode { .. }
            | Kind::ZSuper => false,
            Kind::Binary { operator, .. } => operator != "and" && operator != "or",
            _ => true,
        }
    }

    /// `Parentheses.flat`
    fn parentheses_flat(&mut self, f: impl FnOnce(&mut Self)) {
        if self.parentheses_parent() {
            self.text("(");
            f(self);
            self.text(")");
        } else {
            f(self);
        }
    }

    /// `Parentheses.break`
    fn parentheses_break(&mut self, f: impl FnOnce(&mut Self)) {
        if self.parentheses_parent() {
            self.text("(");
            self.indent(|q| {
                q.breakable_empty();
                f(q);
            });
            self.breakable_empty();
            self.text(")");
        } else {
            f(self);
        }
    }

    fn parentheses_parent(&self) -> bool {
        matches!(
            self.parent_kind(),
            Some(
                Kind::Args { .. }
                    | Kind::Assign { .. }
                    | Kind::Assoc { .. }
                    | Kind::Binary { .. }
                    | Kind::CallNode { .. }
                    | Kind::Defined { .. }
                    | Kind::MAssign { .. }
                    | Kind::OpAssign { .. }
            )
        )
    }

    /// `ConditionalFormatter#format`
    fn format_conditional(&mut self, id: NodeId) {
        let (predicate, statements, consequent, keyword) = self.conditional_parts(id);
        if self.is_modifier(predicate, statements) {
            let statement = self.statements_body(statements)[0];
            if self.contains_assignment(statement) || matches!(self.parent_kind(), Some(Kind::In { .. })) {
                self.group(|q| q.conditional_flat(id));
            } else {
                self.group(|q| {
                    q.if_break(|q| q.conditional_break(id, false), |q| q.conditional_flat(id));
                });
            }
            return;
        }

        if self.ternaryable(id) {
            self.conditional_ternary(id);
            return;
        }

        if self.contains_assignment(predicate) {
            self.conditional_break(id, true);
            return;
        }

        let contains_conditional = {
            let body = self.statements_body(statements);
            body.len() == 1
                && matches!(
                    self.k(body[0]),
                    Kind::IfNode { .. } | Kind::IfOp { .. } | Kind::UnlessNode { .. }
                )
        };
        if consequent.is_some() || self.statements_empty(statements) || contains_conditional {
            self.group(|q| q.conditional_break(id, true));
        } else {
            self.group(|q| {
                q.if_break(
                    |q| q.conditional_break(id, false),
                    |q| {
                        q.parentheses_flat(|q| {
                            q.format(statements);
                            q.text_owned(format!(" {keyword} "));
                            q.format(predicate);
                        });
                    },
                );
            });
        }
    }

    fn conditional_flat(&mut self, id: NodeId) {
        let (predicate, statements, _, keyword) = self.conditional_parts(id);
        let statement = self.statements_body(statements)[0];
        self.parentheses_flat(|q| {
            q.format(statement);
            q.text_owned(format!(" {keyword} "));
            q.format(predicate);
        });
    }

    fn conditional_break(&mut self, id: NodeId, force: bool) {
        let (predicate, statements, consequent, keyword) = self.conditional_parts(id);
        let breakable = |q: &mut Self| {
            if force {
                q.breakable_force();
            } else {
                q.breakable_space();
            }
        };
        self.text_owned(format!("{keyword} "));
        self.nest(keyword.len() as i64 + 1, |q| {
            q.format(predicate);
        });
        if !self.statements_empty(statements) {
            self.indent(|q| {
                breakable(q);
                q.format(statements);
            });
        }
        if let Some(consequent) = consequent {
            breakable(self);
            self.format(consequent);
        }
        breakable(self);
        self.text("end");
    }

    fn conditional_ternary(&mut self, id: NodeId) {
        let (predicate, statements, consequent, keyword) = self.conditional_parts(id);
        let consequent = consequent.unwrap();
        let Kind::Else {
            keyword: else_keyword,
            statements: else_statements,
        } = *self.k(consequent)
        else {
            return;
        };
        self.group(|q| {
            q.if_break(
                |q| {
                    q.text_owned(format!("{keyword} "));
                    q.nest(keyword.len() as i64 + 1, |q| {
                        q.format(predicate);
                    });
                    q.indent(|q| {
                        q.breakable_space();
                        q.format(statements);
                    });
                    q.breakable_space();
                    q.group(|q| {
                        q.format(else_keyword);
                        q.indent(|q| {
                            q.breakable(" ", true, Force::SkipBreakParent);
                            q.format(else_statements);
                        });
                    });
                    q.breakable_space();
                    q.text("end");
                },
                |q| {
                    q.parentheses_flat(|q| {
                        q.format(predicate);
                        q.text(" ? ");
                        let (first, second) = if keyword == "unless" {
                            (else_statements, statements)
                        } else {
                            (statements, else_statements)
                        };
                        q.format(first);
                        q.text(" : ");
                        q.format(second);
                    });
                },
            );
        });
    }

    fn format_ifop(&mut self, id: NodeId) {
        let Kind::IfOp {
            predicate,
            truthy,
            falsy,
        } = *self.k(id)
        else {
            return;
        };
        let force_flat = |q: &Self, node: NodeId| {
            matches!(
                q.k(node),
                Kind::AliasNode { .. }
                    | Kind::Assign { .. }
                    | Kind::Break { .. }
                    | Kind::Command { .. }
                    | Kind::CommandCall { .. }
                    | Kind::Heredoc { .. }
                    | Kind::IfNode { .. }
                    | Kind::IfOp { .. }
                    | Kind::Lambda { .. }
                    | Kind::MAssign { .. }
                    | Kind::Next { .. }
                    | Kind::OpAssign { .. }
                    | Kind::RescueMod { .. }
                    | Kind::ReturnNode { .. }
                    | Kind::Super { .. }
                    | Kind::Undef { .. }
                    | Kind::UnlessNode { .. }
                    | Kind::VoidStmt
                    | Kind::YieldNode { .. }
                    | Kind::ZSuper
            )
        };
        let flat = move |q: &mut Self| {
            q.format(predicate);
            q.text(" ?");
            q.indent(|q| {
                q.breakable_space();
                q.format(truthy);
                q.text(" :");
                q.breakable_space();
                q.format(falsy);
            });
        };
        if matches!(self.parent_kind(), Some(Kind::Paren { .. }))
            || force_flat(self, truthy)
            || force_flat(self, falsy)
        {
            self.group(flat);
            return;
        }
        self.group(|q| {
            q.if_break(
                |q| {
                    q.parentheses_break(|q| {
                        q.text("if ");
                        q.nest("if ".len() as i64, |q| {
                            q.format(predicate);
                        });
                        q.indent(|q| {
                            q.breakable_space();
                            q.format(truthy);
                        });
                        q.breakable_space();
                        q.text("else");
                        q.indent(|q| {
                            q.breakable_space();
                            q.format(falsy);
                        });
                        q.breakable_space();
                        q.text("end");
                    });
                },
                flat,
            );
        });
    }

    fn format_not(&mut self, id: NodeId) {
        let Kind::Not {
            statement,
            parentheses,
        } = *self.k(id)
        else {
            return;
        };
        self.text("not");
        if parentheses {
            self.text("(");
            if let Some(statement) = statement {
                self.format(statement);
            }
            self.text(")");
            return;
        }
        let ternary = match self.grandparent() {
            Some(Entry::Node(gp)) if matches!(self.k(gp), Kind::IfNode { .. } | Kind::UnlessNode { .. }) => {
                // Ternaryable.call(q, grandparent) is evaluated with the
                // current stack, so its own grandparent check looks at our
                // great-grandparent... as upstream does.
                self.ternaryable(gp)
            }
            _ => false,
        };
        if ternary {
            self.if_break(|q| q.text(" "), |q| q.text("("));
            if let Some(statement) = statement {
                self.format(statement);
            }
            self.if_flat(|q| q.text(")"));
        } else {
            self.text(" ");
            if let Some(statement) = statement {
                self.format(statement);
            }
        }
    }

    // ------------------------------------------------------------------
    // Loops
    // ------------------------------------------------------------------

    fn format_loop(&mut self, id: NodeId) {
        let (predicate, statements, keyword) = match *self.k(id) {
            Kind::WhileNode {
                predicate,
                statements,
            } => (predicate, statements, "while"),
            Kind::UntilNode {
                predicate,
                statements,
            } => (predicate, statements, "until"),
            _ => return,
        };
        let modifier = self.is_modifier(predicate, statements);
        let first = self.statements_body(statements).first().copied();
        if modifier
            && let Some(statement) = first
            && (matches!(self.k(statement), Kind::Begin { .. }) || self.contains_assignment(statement))
        {
            self.format(statement);
            self.text_owned(format!(" {keyword} "));
            self.format(predicate);
        } else if self.statements_empty(statements) {
            self.group(|q| {
                q.text_owned(format!("{keyword} "));
                q.nest(keyword.len() as i64 + 1, |q| {
                    q.format(predicate);
                });
                q.breakable_force();
                q.text("end");
            });
        } else if self.contains_assignment(predicate) {
            self.loop_break(keyword, predicate, statements);
            self.break_parent();
        } else {
            self.group(|q| {
                q.if_break(
                    |q| q.loop_break(keyword, predicate, statements),
                    |q| {
                        q.parentheses_flat(|q| {
                            q.format(statements);
                            q.text_owned(format!(" {keyword} "));
                            q.format(predicate);
                        });
                    },
                );
            });
        }
    }

    fn loop_break(&mut self, keyword: &'static str, predicate: NodeId, statements: NodeId) {
        self.text_owned(format!("{keyword} "));
        self.nest(keyword.len() as i64 + 1, |q| {
            q.format(predicate);
        });
        self.indent(|q| {
            q.breakable_empty();
            q.format(statements);
        });
        self.breakable_empty();
        self.text("end");
    }

    fn format_when(&mut self, id: NodeId) {
        let Kind::When {
            arguments,
            statements,
            consequent,
        } = *self.k(id)
        else {
            return;
        };
        let keyword = "when ";
        let parts = match self.k(arguments) {
            Kind::Args { parts } => parts.clone(),
            _ => vec![],
        };
        let has_comments = !self.comments(arguments).is_empty();
        let then = parts
            .last()
            .is_some_and(|&last| matches!(self.k(last), Kind::RangeNode { right: None, .. }));
        self.group(|q| {
            q.group(|q| {
                q.text(keyword);
                q.nest(keyword.len() as i64, |q| {
                    if has_comments {
                        q.format(arguments);
                    } else {
                        for (index, &part) in parts.iter().enumerate() {
                            if index > 0 {
                                q.group(|q| {
                                    q.text(",");
                                    q.breakable_space();
                                });
                            }
                            q.format(part);
                        }
                    }
                    if then {
                        q.text(" then");
                    }
                });
            });
            if !q.statements_empty(statements) {
                q.indent(|q| {
                    q.breakable_force();
                    q.format(statements);
                });
            }
            if let Some(consequent) = consequent {
                q.breakable_force();
                q.format(consequent);
            }
        });
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn format_statements(&mut self, id: NodeId) {
        let body = self.statements_body(id);
        if body.len() == 2
            && matches!(self.k(body[0]), Kind::VoidStmt)
            && matches!(self.k(body[1]), Kind::Comment { .. })
        {
            self.format(body[1]);
            self.break_parent();
            return;
        }

        let parent_is_embexpr = matches!(self.parent_kind(), Some(Kind::StringEmbExpr { .. }));
        let mut line: Option<usize> = None;
        let mut previous: Option<NodeId> = None;
        for statement in body {
            if matches!(self.k(statement), Kind::VoidStmt) {
                continue;
            }
            let loc = *self.ast.loc(statement);
            match line {
                None => {
                    self.format(statement);
                }
                Some(line) if loc.start_line.saturating_sub(line) > 1 && loc.start_line > line => {
                    self.breakable_force();
                    self.breakable_force();
                    self.format(statement);
                }
                Some(line) => {
                    if self.access_control(statement) || previous.is_some_and(|p| self.access_control(p)) {
                        self.breakable_force();
                        self.breakable_force();
                        self.format(statement);
                    } else if loc.start_line != line || !parent_is_embexpr {
                        self.breakable_force();
                        self.format(statement);
                    } else {
                        self.text("; ");
                        self.format(statement);
                    }
                }
            }
            line = Some(loc.end_line);
            previous = Some(statement);
        }
    }

    /// `VCall#access_control?`
    fn access_control(&self, id: NodeId) -> bool {
        match self.k(id) {
            Kind::VCall { value } => matches!(self.value(*value), "private" | "protected" | "public"),
            _ => false,
        }
    }
}

/// The arguments of a Command node.
fn arguments_of_root(q: &Formatter<'_>, node: NodeId) -> NodeId {
    match q.k(node) {
        Kind::Command { arguments, .. } => *arguments,
        _ => node,
    }
}
