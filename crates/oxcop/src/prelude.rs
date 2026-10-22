//! What a cop needs, for `use oxcop::prelude::*;` in a plugin's cop files
//! (`docs/PLUGINS.md`). Built-in cops import from the modules directly.

pub use crate::ast::node_pattern::NodePattern;
pub use crate::ast::{Child, Node, NodeType};
pub use crate::config::Config;
pub use crate::cop::base::{CopError, raise};
pub use crate::cop::mixin::{
    self, allowed_methods, allowed_pattern, configurable_enforced_style, ignored_node, range_help,
    util,
};
pub use crate::cop::{Cop, Corrector, Investigation, Offense, Severity};
pub use crate::path_util;
pub use crate::processed_source::{ProcessedSource, Range};
pub use crate::regex;
pub use serde_yaml_ng::{Mapping, Value};
