//! Port of rubocop-rspec's `lib/rubocop/rspec/language.rb` (the
//! configuration-aware node matchers for the RSpec DSL) and of
//! `lib/rubocop/cop/rspec/base.rb`, whose only behaviour is to point
//! `RuboCop::RSpec::Language.config` at `config['RSpec']['Language']` in
//! `on_new_investigation`.
//!
//! Upstream keeps that configuration in a module-level accessor; here an RSpec
//! cop builds a [`Language`] from its investigation's configuration
//! ([`Language::new`]) and calls the matchers on it:
//!
//! ```ignore
//! let language = Language::new(cx.config);
//! if language.hook(node) { ... }
//! ```

use serde_yaml_ng::{Mapping, Value};

use crate::ast::Node;
use crate::ast::node_pattern::{MatchContext, NodePattern, Val};
use crate::config::Config;

static RSPEC: NodePattern = NodePattern::new("{#explicit_rspec? nil?}");

static EXPLICIT_RSPEC: NodePattern = NodePattern::new("(const {nil? cbase} :RSpec)");

static EXAMPLE_GROUP: NodePattern =
    NodePattern::new("(any_block (send #rspec? #ExampleGroups.all ...) ...)");

static SHARED_GROUP: NodePattern =
    NodePattern::new("(block (send #rspec? #SharedGroups.all ...) ...)");

static SPEC_GROUP: NodePattern = NodePattern::new(
    "(any_block (send #rspec?
         {#SharedGroups.all #ExampleGroups.all}
      ...) ...)",
);

static EXAMPLE_GROUP_WITH_BODY: NodePattern =
    NodePattern::new("(block (send #rspec? #ExampleGroups.all ...) args !nil?)");

static EXAMPLE: NodePattern = NodePattern::new("(block (send nil? #Examples.all ...) ...)");

static HOOK: NodePattern = NodePattern::new("(any_block (send nil? #Hooks.all ...) ...)");

static LET: NodePattern = NodePattern::new(
    "{
      (block (send nil? #Helpers.all ...) ...)
      (send nil? #Helpers.all _ block_pass)
    }",
);

static INCLUDE: NodePattern = NodePattern::new(
    "{
      (block (send nil? #Includes.all ...) ...)
      (send nil? #Includes.all ...)
    }",
);

static SUBJECT: NodePattern = NodePattern::new("(block (send nil? #Subjects.all ...) ...)");

/// `HookScopes::ALL`.
pub const HOOK_SCOPES: &[&str] = &["each", "example", "context", "all", "suite"];

/// `Runners::ALL`.
pub const RUNNERS: &[&str] = &["to", "to_not", "not_to"];

/// A concept of the RSpec DSL, as `Language::ALL.all(element)` returns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Concept {
    ErrorMatchers,
    ExampleGroups,
    Examples,
    Expectations,
    Helpers,
    Hooks,
    Includes,
    Runners,
    SharedGroups,
    Subjects,
}

/// `RuboCop::RSpec::Language` with its `config` set: the `RSpec: Language:`
/// section of the configuration.
#[derive(Clone, Copy)]
pub struct Language<'c> {
    config: Option<&'c Mapping>,
}

impl<'c> Language<'c> {
    /// `RuboCop::RSpec::Language.config = config['RSpec']['Language']`
    /// (`RuboCop::Cop::RSpec::Base#on_new_investigation`).
    pub fn new(config: &'c Config) -> Self {
        Self::from_mapping(
            config
                .for_department("RSpec")
                .get("Language")
                .and_then(Value::as_mapping),
        )
    }

    /// A language for the given `RSpec: Language:` mapping. Without one,
    /// nothing is part of the DSL (upstream would fail on `nil[]`).
    pub fn from_mapping(config: Option<&'c Mapping>) -> Self {
        Self { config }
    }

    // ---- node matchers ------------------------------------------------------

    /// `rspec?(node)`: `RSpec`, `::RSpec` or no receiver.
    pub fn rspec(&self, val: Val<'_>) -> bool {
        self.matches(&RSPEC, val)
    }

    /// `explicit_rspec?(node)`.
    pub fn explicit_rspec(&self, val: Val<'_>) -> bool {
        self.matches(&EXPLICIT_RSPEC, val)
    }

    /// `example_group?(node)`.
    pub fn example_group(&self, node: Node<'_>) -> bool {
        self.matches(&EXAMPLE_GROUP, Val::Node(node))
    }

    /// `shared_group?(node)`.
    pub fn shared_group(&self, node: Node<'_>) -> bool {
        self.matches(&SHARED_GROUP, Val::Node(node))
    }

    /// `spec_group?(node)`.
    pub fn spec_group(&self, node: Node<'_>) -> bool {
        self.matches(&SPEC_GROUP, Val::Node(node))
    }

    /// `example_group_with_body?(node)`.
    pub fn example_group_with_body(&self, node: Node<'_>) -> bool {
        self.matches(&EXAMPLE_GROUP_WITH_BODY, Val::Node(node))
    }

    /// `example?(node)`.
    pub fn example(&self, node: Node<'_>) -> bool {
        self.matches(&EXAMPLE, Val::Node(node))
    }

    /// `hook?(node)`.
    pub fn hook(&self, node: Node<'_>) -> bool {
        self.matches(&HOOK, Val::Node(node))
    }

    /// `let?(node)`.
    pub fn let_(&self, node: Node<'_>) -> bool {
        self.matches(&LET, Val::Node(node))
    }

    /// `include?(node)`.
    pub fn include(&self, node: Node<'_>) -> bool {
        self.matches(&INCLUDE, Val::Node(node))
    }

    /// `subject?(node)`.
    pub fn subject(&self, node: Node<'_>) -> bool {
        self.matches(&SUBJECT, Val::Node(node))
    }

    /// Matches one of this module's patterns, with its `#method` calls
    /// resolved against the configuration.
    pub fn matches(&self, pattern: &NodePattern, val: Val<'_>) -> bool {
        let call = |name: &str, val: &Val<'_>, _: &[Val<'_>]| self.call(name, *val);
        let ctx = MatchContext {
            call: Some(&call),
            ..Default::default()
        };
        pattern.match_val(val, &ctx).is_some()
    }

    /// The `#method` calls of the patterns: `#rspec?`, `#ExampleGroups.all`
    /// and so on. A pattern of a cop that calls these matchers and the
    /// concept functions can delegate to it.
    pub fn call(&self, name: &str, val: Val<'_>) -> bool {
        match name {
            "rspec?" => self.rspec(val),
            "explicit_rspec?" => self.explicit_rspec(val),
            "example_group?" => val.node().is_some_and(|n| self.example_group(n)),
            "shared_group?" => val.node().is_some_and(|n| self.shared_group(n)),
            "spec_group?" => val.node().is_some_and(|n| self.spec_group(n)),
            "example_group_with_body?" => {
                val.node().is_some_and(|n| self.example_group_with_body(n))
            }
            "example?" => val.node().is_some_and(|n| self.example(n)),
            "hook?" => val.node().is_some_and(|n| self.hook(n)),
            "let?" => val.node().is_some_and(|n| self.let_(n)),
            "include?" => val.node().is_some_and(|n| self.include(n)),
            "subject?" => val.node().is_some_and(|n| self.subject(n)),
            _ => {
                let Some((concept, method)) = name.split_once('.') else {
                    panic!("RSpec::Language: unknown node pattern function #{name}");
                };
                // `element.to_s`: the matchers only pass method names.
                let Some(element) = val.sym() else {
                    return false;
                };
                self.concept(concept, method, element)
            }
        }
    }

    /// `<Concept>.<method>(element)`, e.g. `ExampleGroups.focused`.
    pub fn concept(&self, concept: &str, method: &str, element: &str) -> bool {
        match (concept, method) {
            ("ErrorMatchers", "all") => self.error_matchers(element),
            ("ExampleGroups", "all") => self.example_groups(element),
            ("ExampleGroups", "regular") => self.example_groups_regular(element),
            ("ExampleGroups", "focused") => self.example_groups_focused(element),
            ("ExampleGroups", "skipped") => self.example_groups_skipped(element),
            ("Examples", "all") => self.examples(element),
            ("Examples", "regular") => self.examples_regular(element),
            ("Examples", "focused") => self.examples_focused(element),
            ("Examples", "skipped") => self.examples_skipped(element),
            ("Examples", "pending") => self.examples_pending(element),
            ("Expectations", "all") => self.expectations(element),
            ("Helpers", "all") => self.helpers(element),
            ("Hooks", "all") => self.hooks(element),
            ("HookScopes", "all") => HOOK_SCOPES.contains(&element),
            ("Includes", "all") => self.includes(element),
            ("Includes", "examples") => self.includes_examples(element),
            ("Includes", "context") => self.includes_context(element),
            ("Runners", "all") => RUNNERS.contains(&element),
            ("SharedGroups", "all") => self.shared_groups(element),
            ("SharedGroups", "examples") => self.shared_groups_examples(element),
            ("SharedGroups", "context") => self.shared_groups_context(element),
            ("Subjects", "all") => self.subjects(element),
            ("ALL", "all") => self.all(element).is_some(),
            _ => panic!("RSpec::Language: unknown node pattern function #{concept}.{method}"),
        }
    }

    // ---- concepts -------------------------------------------------------------

    /// `Language.config.dig(*path).include?(element.to_s)`.
    fn configured(&self, path: &[&str], element: &str) -> bool {
        let mut value = self.config;
        let (last, parents) = path.split_last().expect("a configuration path");
        for key in parents {
            value = value.and_then(|m| m.get(*key)).and_then(Value::as_mapping);
        }
        match value.and_then(|m| m.get(*last)) {
            Some(Value::Sequence(items)) => items.iter().any(|item| item.as_str() == Some(element)),
            _ => false,
        }
    }

    /// `ErrorMatchers.all(element)`.
    pub fn error_matchers(&self, element: &str) -> bool {
        self.configured(&["ErrorMatchers"], element)
    }

    /// `ExampleGroups.all(element)`.
    pub fn example_groups(&self, element: &str) -> bool {
        self.example_groups_regular(element)
            || self.example_groups_skipped(element)
            || self.example_groups_focused(element)
    }

    pub fn example_groups_regular(&self, element: &str) -> bool {
        self.configured(&["ExampleGroups", "Regular"], element)
    }

    pub fn example_groups_focused(&self, element: &str) -> bool {
        self.configured(&["ExampleGroups", "Focused"], element)
    }

    pub fn example_groups_skipped(&self, element: &str) -> bool {
        self.configured(&["ExampleGroups", "Skipped"], element)
    }

    /// `Examples.all(element)`.
    pub fn examples(&self, element: &str) -> bool {
        self.examples_regular(element)
            || self.examples_focused(element)
            || self.examples_skipped(element)
            || self.examples_pending(element)
    }

    pub fn examples_regular(&self, element: &str) -> bool {
        self.configured(&["Examples", "Regular"], element)
    }

    pub fn examples_focused(&self, element: &str) -> bool {
        self.configured(&["Examples", "Focused"], element)
    }

    pub fn examples_skipped(&self, element: &str) -> bool {
        self.configured(&["Examples", "Skipped"], element)
    }

    pub fn examples_pending(&self, element: &str) -> bool {
        self.configured(&["Examples", "Pending"], element)
    }

    /// `Expectations.all(element)`.
    pub fn expectations(&self, element: &str) -> bool {
        self.configured(&["Expectations"], element)
    }

    /// `Helpers.all(element)`.
    pub fn helpers(&self, element: &str) -> bool {
        self.configured(&["Helpers"], element)
    }

    /// `Hooks.all(element)`.
    pub fn hooks(&self, element: &str) -> bool {
        self.configured(&["Hooks"], element)
    }

    /// `Includes.all(element)`.
    pub fn includes(&self, element: &str) -> bool {
        self.includes_examples(element) || self.includes_context(element)
    }

    pub fn includes_examples(&self, element: &str) -> bool {
        self.configured(&["Includes", "Examples"], element)
    }

    pub fn includes_context(&self, element: &str) -> bool {
        self.configured(&["Includes", "Context"], element)
    }

    /// `SharedGroups.all(element)`.
    pub fn shared_groups(&self, element: &str) -> bool {
        self.shared_groups_examples(element) || self.shared_groups_context(element)
    }

    pub fn shared_groups_examples(&self, element: &str) -> bool {
        self.configured(&["SharedGroups", "Examples"], element)
    }

    pub fn shared_groups_context(&self, element: &str) -> bool {
        self.configured(&["SharedGroups", "Context"], element)
    }

    /// `Subjects.all(element)`.
    pub fn subjects(&self, element: &str) -> bool {
        self.configured(&["Subjects"], element)
    }

    /// `ALL.all(element)`: the first concept `element` belongs to (used by
    /// the Dialect and DescribeClass cops).
    pub fn all(&self, element: &str) -> Option<Concept> {
        [
            (Concept::ErrorMatchers, self.error_matchers(element)),
            (Concept::ExampleGroups, self.example_groups(element)),
            (Concept::Examples, self.examples(element)),
            (Concept::Expectations, self.expectations(element)),
            (Concept::Helpers, self.helpers(element)),
            (Concept::Hooks, self.hooks(element)),
            (Concept::Includes, self.includes(element)),
            (Concept::Runners, RUNNERS.contains(&element)),
            (Concept::SharedGroups, self.shared_groups(element)),
            (Concept::Subjects, self.subjects(element)),
        ]
        .into_iter()
        .find_map(|(concept, member)| member.then_some(concept))
    }
}
