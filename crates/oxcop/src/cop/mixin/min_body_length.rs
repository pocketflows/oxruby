//! Port of `lib/rubocop/cop/mixin/min_body_length.rb`.

use crate::ast::Node;
use crate::cop::Investigation;

/// `min_body_length?(node)`.
pub fn min_body_length(cx: &Investigation<'_, '_>, node: Node<'_>) -> bool {
    let ps = cx.processed_source;
    let (Some(end), Some(keyword)) = (node.loc().end, node.loc().keyword) else {
        return false;
    };
    let lines = ps.line_of(end.begin_pos) as i64 - ps.line_of(keyword.begin_pos) as i64;
    lines > configured_min_body_length(cx)
}

fn configured_min_body_length(cx: &Investigation<'_, '_>) -> i64 {
    match cx.cop_config_value("MinBodyLength") {
        None | Some(serde_yaml_ng::Value::Null) => 1,
        Some(value) => match value.as_i64() {
            Some(length) if length > 0 => length,
            _ => panic!("MinBodyLength needs to be a positive integer!"),
        },
    }
}
