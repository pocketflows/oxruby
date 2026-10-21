//! Port of `lib/rubocop/cop/commissioner.rb` and rubocop-ast's
//! `Traversal`: one walk over the AST that calls each cop's callbacks.

use std::collections::HashMap;

use crate::ast::{Node, NodeType};
use crate::comment_config::CommentConfig;
use crate::config::Config;
use crate::cop::base::CopOptions;
use crate::cop::{Cop, Corrector, Investigation, Offense};
use crate::processed_source::ProcessedSource;

pub struct CopReport {
    pub cop_name: &'static str,
    pub incompatible_with: &'static [&'static str],
    pub offenses: Vec<Offense>,
    pub corrector: Option<Corrector>,
    pub errors: Vec<String>,
}

const TYPE_COUNT: usize = NodeType::ALL.len();

/// Node types whose children `Traversal` does not visit.
fn no_child_nodes(ty: NodeType) -> bool {
    use NodeType::*;
    matches!(
        ty,
        True | False
            | Nil
            | SelfNode
            | Cbase
            | Zsuper
            | Redo
            | Retry
            | ForwardArgs
            | ForwardedArgs
            | MatchNilPattern
            | ForwardArg
            | ForwardedRestarg
            | ForwardedKwrestarg
            | Lambda
            | EmptyElse
            | Kwnilarg
            | Restarg
            | Kwrestarg
            | Int
            | Float
            | Complex
            | Rational
            | Str
            | Sym
            | Lvar
            | Ivar
            | Cvar
            | Gvar
            | NthRef
            | BackRef
            | Arg
            | Blockarg
            | Shadowarg
            | Kwarg
            | MatchVar
    )
}

#[derive(Default)]
struct Callbacks {
    /// Per node type: cops with an unrestricted callback.
    on: Vec<Vec<usize>>,
    after: Vec<Vec<usize>>,
    /// `send`/`csend` callbacks restricted by method name.
    restricted_on: HashMap<&'static str, Vec<usize>>,
    restricted_after: HashMap<&'static str, Vec<usize>>,
}

pub fn investigate(
    cops: &mut [Box<dyn Cop>],
    processed_source: &ProcessedSource<'_>,
    config: &Config,
    comment_config: &CommentConfig,
    options: CopOptions,
) -> Vec<CopReport> {
    let mut cxs: Vec<Investigation<'_, '_>> = cops
        .iter()
        .map(|cop| {
            Investigation::new(
                cop.as_ref(),
                processed_source,
                config,
                comment_config,
                options,
            )
        })
        .collect();

    if processed_source.valid_syntax() {
        for (cop, cx) in cops.iter_mut().zip(cxs.iter_mut()) {
            cop.on_new_investigation(cx);
        }
        if let Some(root) = processed_source.ast() {
            crate::cop::variable_force::investigate_cops(cops, &mut cxs, root);
        }
        if let Some(root) = processed_source.ast() {
            let callbacks = build_callbacks(cops);
            walk(root, cops, &mut cxs, &callbacks);
        }
        for (cop, cx) in cops.iter_mut().zip(cxs.iter_mut()) {
            cop.on_investigation_end(cx);
        }
    } else {
        for (cop, cx) in cops.iter_mut().zip(cxs.iter_mut()) {
            cop.on_other_file(cx);
        }
    }

    cops.iter()
        .zip(cxs)
        .map(|(cop, cx)| {
            let (offenses, corrector, errors) = cx.into_parts();
            CopReport {
                cop_name: cop.name(),
                incompatible_with: cop.autocorrect_incompatible_with(),
                offenses,
                corrector,
                errors,
            }
        })
        .collect()
}

fn build_callbacks(cops: &[Box<dyn Cop>]) -> Callbacks {
    let mut callbacks = Callbacks {
        on: vec![Vec::new(); TYPE_COUNT],
        after: vec![Vec::new(); TYPE_COUNT],
        ..Default::default()
    };
    for (index, cop) in cops.iter().enumerate() {
        let restrictions = cop.restrict_on_send();
        for &ty in cop.node_types() {
            if restrictions.is_empty() || !matches!(ty, NodeType::Send | NodeType::Csend) {
                callbacks.on[ty as usize].push(index);
            } else if ty == NodeType::Send {
                for &name in restrictions {
                    callbacks.restricted_on.entry(name).or_default().push(index);
                }
            }
        }
        for &ty in cop.after_node_types() {
            if restrictions.is_empty() || !matches!(ty, NodeType::Send | NodeType::Csend) {
                callbacks.after[ty as usize].push(index);
            } else if ty == NodeType::Send {
                for &name in restrictions {
                    callbacks
                        .restricted_after
                        .entry(name)
                        .or_default()
                        .push(index);
                }
            }
        }
    }
    callbacks
}

/// Cops that restrict `send` callbacks also get `csend` nodes for those
/// names when they handle `csend`.
fn restricted_targets<'c>(
    cops: &[Box<dyn Cop>],
    map: &'c HashMap<&'static str, Vec<usize>>,
    node: Node<'_>,
    after: bool,
) -> impl Iterator<Item = usize> + 'c {
    let name = match node.children().get(1) {
        Some(crate::ast::Child::Sym(name)) => Some(name.as_str()),
        _ => None,
    };
    let ty = node.ty();
    let wanted: Vec<usize> = name
        .and_then(|name| map.get(name))
        .map(|list| {
            list.iter()
                .copied()
                .filter(|&i| {
                    let types = if after {
                        cops[i].after_node_types()
                    } else {
                        cops[i].node_types()
                    };
                    types.contains(&ty)
                })
                .collect()
        })
        .unwrap_or_default();
    wanted.into_iter()
}

fn walk<'a>(
    root: Node<'a>,
    cops: &mut [Box<dyn Cop>],
    cxs: &mut [Investigation<'a, '_>],
    callbacks: &Callbacks,
) {
    // An explicit stack instead of recursion: (node, children visited?).
    let mut stack: Vec<(Node<'a>, bool)> = vec![(root, false)];
    while let Some((node, visited)) = stack.pop() {
        let ty = node.ty();
        let is_send = matches!(ty, NodeType::Send | NodeType::Csend);
        if !visited {
            for &i in &callbacks.on[ty as usize] {
                cops[i].on_node(&mut cxs[i], node);
            }
            if is_send && !callbacks.restricted_on.is_empty() {
                let targets: Vec<usize> =
                    restricted_targets(cops, &callbacks.restricted_on, node, false).collect();
                for i in targets {
                    cops[i].on_node(&mut cxs[i], node);
                }
            }
            stack.push((node, true));
            if !no_child_nodes(ty) {
                let children: Vec<Node<'a>> = node.child_nodes().collect();
                for child in children.into_iter().rev() {
                    stack.push((child, false));
                }
            }
        } else {
            for &i in &callbacks.after[ty as usize] {
                cops[i].after_node(&mut cxs[i], node);
            }
            if is_send && !callbacks.restricted_after.is_empty() {
                let targets: Vec<usize> =
                    restricted_targets(cops, &callbacks.restricted_after, node, true).collect();
                for i in targets {
                    cops[i].after_node(&mut cxs[i], node);
                }
            }
        }
    }
}
