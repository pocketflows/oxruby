//! Port of `lib/rubocop/cop/metrics/utils/repeated_csend_discount.rb`:
//! identifies repetitions of `&.` on the same variable.
//!
//! ```ruby
//! my_var&.foo
//! my_var&.bar # => repeated
//! my_var = baz # => reset
//! my_var&.qux # => not repeated
//! ```

use std::collections::HashMap;

use crate::ast::{Node, NodeId, NodeType};

#[derive(Default)]
pub struct RepeatedCsendDiscount {
    repeated_csend: HashMap<String, NodeId>,
}

impl RepeatedCsendDiscount {
    pub fn reset_repeated_csend(&mut self) {
        self.repeated_csend.clear();
    }

    pub fn discount_for_repeated_csend(&mut self, csend_node: Node<'_>) -> bool {
        let Some(receiver) = csend_node.receiver().filter(|r| r.is(NodeType::Lvar)) else {
            return false;
        };
        let var_name = receiver.name().unwrap_or("").to_owned();
        match self.repeated_csend.get(&var_name) {
            None => {
                self.repeated_csend.insert(var_name, csend_node.id());
                false
            }
            Some(&seen) => seen != csend_node.id(),
        }
    }

    pub fn reset_on_lvasgn(&mut self, node: Node<'_>) {
        if let Some(name) = node.name() {
            self.repeated_csend.remove(name);
        }
    }
}
