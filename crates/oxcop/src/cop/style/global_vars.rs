//! Port of `lib/rubocop/cop/style/global_vars.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG: &str = "Do not introduce global variables.";

/// Built-in global variables and their English aliases.
const BUILT_IN_VARS: &[&str] = &[
    "$:", "$LOAD_PATH", "$\"", "$LOADED_FEATURES", "$0", "$PROGRAM_NAME", "$!", "$ERROR_INFO",
    "$@", "$ERROR_POSITION", "$;", "$FS", "$FIELD_SEPARATOR", "$,", "$OFS",
    "$OUTPUT_FIELD_SEPARATOR", "$/", "$RS", "$INPUT_RECORD_SEPARATOR", "$\\", "$ORS",
    "$OUTPUT_RECORD_SEPARATOR", "$.", "$NR", "$INPUT_LINE_NUMBER", "$_", "$LAST_READ_LINE",
    "$>", "$DEFAULT_OUTPUT", "$<", "$DEFAULT_INPUT", "$$", "$PID", "$PROCESS_ID", "$?",
    "$CHILD_STATUS", "$~", "$LAST_MATCH_INFO", "$=", "$IGNORECASE", "$*", "$ARGV", "$&",
    "$MATCH", "$`", "$PREMATCH", "$'", "$POSTMATCH", "$+", "$LAST_PAREN_MATCH", "$stdin",
    "$stdout", "$stderr", "$DEBUG", "$FILENAME", "$VERBOSE", "$SAFE", "$-0", "$-a", "$-d",
    "$-F", "$-i", "$-I", "$-l", "$-p", "$-v", "$-w", "$CLASSPATH", "$JRUBY_VERSION",
    "$JRUBY_REVISION", "$ENV_JAVA",
];

#[derive(Default)]
pub struct GlobalVars;

impl Cop for GlobalVars {
    fn name(&self) -> &'static str {
        "Style/GlobalVars"
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Gvar, NodeType::Gvasgn]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(name) = node.name() else {
            return;
        };
        if BUILT_IN_VARS.contains(&name) || cx.cop_config_list("AllowedVariables").contains(&name) {
            return;
        }
        cx.add_offense(node.loc().name.unwrap_or(node.source_range()), MSG);
    }
}
