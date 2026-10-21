//! Port of `lib/rubocop/cop/style/date_time.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Node, NodeType};
use crate::cop::{Cop, Investigation};

const CLASS_MSG: &str = "Prefer `Time` over `DateTime`.";
const COERCION_MSG: &str = "Do not use `#to_datetime`.";

static DATE_TIME: NodePattern = NodePattern::new("(call (const {nil? (cbase)} :DateTime) ...)");

static HISTORIC_DATE: NodePattern =
    NodePattern::new("(call _ _ _ (const (const {nil? (cbase)} :Date) _))");

static TO_DATETIME: NodePattern = NodePattern::new("(call !nil? :to_datetime)");

#[derive(Default)]
pub struct DateTime;

impl Cop for DateTime {
    fn name(&self) -> &'static str {
        "Style/DateTime"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send, NodeType::Csend]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let disallow_coercion = !cx.cop_config_bool("AllowCoercion", false);
        let to_datetime = TO_DATETIME.is_match(node);
        if !(DATE_TIME.is_match(node) || (to_datetime && disallow_coercion)) {
            return;
        }
        if HISTORIC_DATE.is_match(node) {
            return;
        }
        let message = if to_datetime { COERCION_MSG } else { CLASS_MSG };
        cx.add_offense_with_correction(node.source_range(), message, |corrector| {
            if to_datetime {
                return;
            }
            corrector.replace(node.receiver().unwrap().loc().name.unwrap(), "Time");
        });
    }
}
