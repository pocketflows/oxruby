# frozen_string_literal: true

module Oxcop
  module Spec
    # What `build_index(uri => source)` returns when the specs run against
    # oxcop: the sources themselves. The cop helper sends them with each
    # investigation of a cop whose `project_index` is one of these
    # (`--project-index-sources`), and the binary builds its own rubydex
    # graph from them, as the spec would have in Ruby.
    class ProjectIndex
      attr_reader :sources

      def initialize(sources)
        @sources = sources.map { |uri, source| [uri.to_s, source.to_s] }
      end
    end

    # The cops whose index-aware code is ported. The `:project_index`
    # examples of the others stay pending until it is.
    INDEX_AWARE_COPS = %w[
      Lint/ArgumentMismatch
      Lint/DeprecatedReference
      Lint/NameTypo
      Lint/ConstantResolution
      Lint/InheritException
      Lint/MissingSuper
      Lint/SuperArgumentMismatch
      Lint/UnusedPrivateMethod
      Naming/AccessorMethodName
      Naming/PredicatePrefix
      Style/ClassAndModuleChildren
      Style/Documentation
      Style/MissingRespondToMissing
      Style/RedundantConstantBase
      Style/StaticClass
    ].freeze

    # Examples that drive the Ruby cop's internals with a real
    # `Rubydex::Graph` instead of investigating through the binary, so
    # they can't run here (see docs/agents/index.md). Lint/UnusedPrivateMethod's
    # reference-name cache is `ProjectIndex#reference_names` in oxcop: one
    # computation per index, shared by every cop instance.
    RUBY_INTERNALS_EXAMPLES = [
      "RuboCop::Cop::Lint::UnusedPrivateMethod with a project index the reference-name cache"
    ].freeze

    # Overrides spec/support/project_index_metadata.rb's helpers for
    # `:project_index` examples (included after it, so these win).
    module ProjectIndexHelpers
      def build_index(sources)
        ProjectIndex.new(sources)
      end

      # `RuboCop::Runner` on a fixture project, as the oxcop binary: the same
      # `.rubocop.yml` as `options_config` (`-c`) and the JSON formatter's
      # output. oxcop has no result cache, so `cache_root` changes nothing.
      def project_index_offenses(tmpdir, cache_root: nil, paths: nil) # rubocop:disable Lint/UnusedMethodArgument
        output_path = File.join(tmpdir, "offenses.json")
        stdout, = Dir.chdir(tmpdir) do
          Binary.send(:run, "-c", File.join(tmpdir, ".rubocop.yml"), "--format", "json", *(paths || [tmpdir]))
        end
        File.write(output_path, stdout)
        JSON.load_file(output_path).fetch("files").flat_map do |file|
          file["offenses"].map { |offense| offense.merge("path" => file["path"]) }
        end
      end
    end
  end
end

# The binary links the rubydex crate, so the index is always available.
RuboCop::ProjectIndexLoader.singleton_class.prepend(
  Module.new do
    def available?
      true
    end
  end
)

RSpec.configure do |config|
  config.include Oxcop::Spec::ProjectIndexHelpers, :project_index

  config.before(:each, :project_index) do |example|
    cop_class = example.metadata[:described_class]
    next unless cop_class.is_a?(Class) && cop_class < RuboCop::Cop::Base

    if Oxcop::Spec::RUBY_INTERNALS_EXAMPLES.any? { |prefix| example.full_description.start_with?(prefix) }
      skip "oxcop: drives the Ruby cop's internals with a rubydex graph, which the shim doesn't build."
    end
    next if Oxcop::Spec::INDEX_AWARE_COPS.include?(cop_class.cop_name)

    skip "oxcop: #{cop_class.cop_name}'s project-index code is not ported yet."
  end
end
