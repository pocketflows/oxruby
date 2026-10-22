# frozen_string_literal: true

require "rubocop-rails"
require "rubocop-rspec"
require "rubocop/rspec/shared_contexts/default_rspec_language_config_context"

module Oxcop
  module Spec
    # The vendored plugin cop specs (spec/rubocop/cop/rails/, .../rspec/).
    #
    # RuboCop's CopHelper integrates every bundled plugin into the global
    # default configuration once, before the first example. The plugins
    # change core cops' defaults (rubocop-rails sets Lint/SafeNavigationChain's
    # AllowedMethods, for one), so here each plugin's specs get a default
    # configuration with only that plugin integrated, and every other spec
    # keeps RuboCop's own.
    module Plugins
      # spec/rubocop/cop/<directory>/ => plugin gem
      DIRECTORIES = { "rails" => "rubocop-rails", "rspec" => "rubocop-rspec" }.freeze

      class << self
        def plugin_for(file_path)
          directory = file_path[%r{/spec/rubocop/cop/([^/]+)/}, 1]
          DIRECTORIES[directory]
        end

        # `ConfigLoader.default_configuration` with only `plugin` integrated.
        def default_configuration(plugin)
          @default_configurations ||= {}
          @default_configurations[plugin] ||= begin
            core = RuboCop::ConfigLoader.default_configuration
            RuboCop::ConfigLoader.default_configuration = nil
            RuboCop::Plugin.integrate_plugins(RuboCop::Config.new, [plugin])
            RuboCop::ConfigLoader.default_configuration
          ensure
            RuboCop::ConfigLoader.default_configuration = core
          end
        end

        def with_default_configuration(plugin)
          core = RuboCop::ConfigLoader.default_configuration
          RuboCop::ConfigLoader.default_configuration = default_configuration(plugin)
          yield
        ensure
          RuboCop::ConfigLoader.default_configuration = core
        end
      end
    end
  end
end

CopHelper.integrated_plugins = true

RSpec.configure do |config|
  config.define_derived_metadata do |metadata|
    metadata[:oxcop_plugin] ||= Oxcop::Spec::Plugins.plugin_for(metadata[:file_path].to_s)
  end

  config.around do |example|
    plugin = example.metadata[:oxcop_plugin]
    next example.run unless plugin

    Oxcop::Spec::Plugins.with_default_configuration(plugin) { example.run }
  end

  # rubocop-rspec's spec_helper: every cop spec gets the `config` shared
  # context, the default RSpec/Language configuration, the smoke tests, and
  # its ExpectOffense (whose default file name ends in `_spec.rb`).
  config.define_derived_metadata(file_path: %r{/spec/rubocop/cop/rspec/(?!mixin/)}) do |metadata|
    metadata[:type] = :cop_spec
  end
  config.define_derived_metadata(type: :cop_spec) do |metadata|
    metadata[:config] = true
  end
  config.include(ExpectOffense, type: :cop_spec)
  config.include_context "with default RSpec/Language config", type: :cop_spec
  config.include_context "smoke test", type: :cop_spec
end
