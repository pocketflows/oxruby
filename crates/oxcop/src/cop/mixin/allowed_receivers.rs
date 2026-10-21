//! Port of `lib/rubocop/cop/mixin/allowed_receivers.rb`.

use crate::ast::{Node, NodeType};
use crate::cop::Investigation;

/// `allowed_receiver?(receiver)`.
pub fn allowed_receiver(cx: &Investigation<'_, '_>, receiver: Node<'_>) -> bool {
    let receiver_name = receiver_name(receiver);
    allowed_receivers(cx).contains(&receiver_name.as_str())
}

/// `receiver_name(receiver)`.
pub fn receiver_name(receiver: Node<'_>) -> String {
    if let Some(inner) = receiver.receiver()
        && !inner.is(NodeType::Const)
    {
        return receiver_name(inner);
    }
    if receiver.is_send() {
        match receiver.receiver() {
            Some(inner) => format!(
                "{}.{}",
                receiver_name(inner),
                receiver.method_name().unwrap_or("")
            ),
            None => receiver.method_name().unwrap_or("").to_owned(),
        }
    } else {
        receiver.source().to_owned()
    }
}

/// `allowed_receivers`.
pub fn allowed_receivers<'a>(cx: &Investigation<'a, '_>) -> Vec<&'a str> {
    cx.cop_config_list("AllowedReceivers")
}
