//! Port of `lib/rubocop/cop/bundler/insecure_protocol_source.rb`.

use crate::ast::node_pattern::NodePattern;
use crate::ast::{Child, Node, NodeType};
use crate::cop::{Cop, Investigation};

const MSG_HTTP_PROTOCOL: &str = "Use `https://rubygems.org` instead of `http://rubygems.org`.";

static INSECURE_PROTOCOL_SOURCE: NodePattern = NodePattern::new(
    r#"(send nil? :source
       ${(sym :gemcutter) (sym :rubygems) (sym :rubyforge) (:str "http://rubygems.org")})"#,
);

#[derive(Default)]
pub struct InsecureProtocolSource;

impl Cop for InsecureProtocolSource {
    fn name(&self) -> &'static str {
        "Bundler/InsecureProtocolSource"
    }

    fn support_autocorrect(&self) -> bool {
        true
    }

    fn node_types(&self) -> &'static [NodeType] {
        &[NodeType::Send]
    }

    fn restrict_on_send(&self) -> &'static [&'static str] {
        &["source"]
    }

    fn on_node<'a>(&mut self, cx: &mut Investigation<'a, '_>, node: Node<'a>) {
        let Some(captures) = INSECURE_PROTOCOL_SOURCE.match_node(node) else {
            return;
        };
        let source_node = captures[0].node().unwrap();
        let source = match source_node.children().first() {
            Some(Child::Sym(s) | Child::Str(s)) => s.as_str(),
            _ => "",
        };
        let use_http_protocol = source == "http://rubygems.org";
        if cx.cop_config_bool("AllowHttpProtocol", true) && use_http_protocol {
            return;
        }
        let message = if use_http_protocol {
            MSG_HTTP_PROTOCOL.to_owned()
        } else {
            format!(
                "The source `:{source}` is deprecated because HTTP requests are insecure. \
                 Please change your source to 'https://rubygems.org' if possible, or \
                 'http://rubygems.org' if not."
            )
        };
        cx.add_offense_with_correction(source_node.source_range(), &message, |corrector| {
            corrector.replace(source_node.source_range(), "'https://rubygems.org'");
        });
    }
}
