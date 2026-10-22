# frozen_string_literal: true

# Runs RuboCop's own cop specs (vendored under spec/rubocop/cop/) against the
# oxcop binary. The real rubocop gem supplies the spec DSL, cop constants and
# configuration; spec/oxcop/cop_helper.rb swaps the investigation itself for
# a call to oxcop.

require "rainbow"
Rainbow.enabled = false

require "rubocop"
require "rubocop/rspec/support"

require_relative "core_ext/string"

# Upstream support files (shared examples and helpers), kept verbatim.
Dir["#{__dir__}/support/**/*.rb"].sort.each { |f| require f }

require_relative "oxcop/binary"
require_relative "oxcop/cop_helper"
require_relative "oxcop/project_index"
require_relative "oxcop/plugins"

RSpec.configure do |config|
  config.include FileHelper
  config.include Oxcop::Spec::CopHelper

  config.filter_run_when_matching :focus
  config.example_status_persistence_file_path = "spec/examples.txt"
  config.order = :random
  Kernel.srand config.seed

  config.filter_run_excluding broken_on: :ruby_head if ENV["CI_RUBY_VERSION"] == "head"

  # oxcop always parses with Prism, which handles Ruby 3.3+ syntax only;
  # upstream skips these examples under its Prism engine (`unsupported_on:`).
  config.filter_run_excluding unsupported_on: :prism

  # Examples for cops that have not been ported yet are filtered out rather
  # than run; the scoreboard printed at the end tracks the remaining work.
  config.define_derived_metadata do |metadata|
    cop_class = metadata[:described_class]
    next unless cop_class.is_a?(Class) && cop_class < RuboCop::Cop::Base

    metadata[:unported] = true unless Oxcop::Spec::Binary.ported?(cop_class.cop_name)
  end
  config.filter_run_excluding :unported unless ENV["OXCOP_RUN_UNPORTED"]

  # Some upstream specs drive RuboCop's own Commissioner or cop methods
  # instead of expect_offense; those pass without testing oxcop at all.
  bypassed = Hash.new(0)
  config.around do |example|
    before = Oxcop::Spec::Binary.investigations.to_i
    example.run
    cop_class = example.metadata[:described_class]
    if cop_class.is_a?(Class) && cop_class < RuboCop::Cop::Base && Oxcop::Spec::Binary.investigations.to_i == before
      bypassed[cop_class.cop_name] += 1
    end
  end

  config.after(:suite) do
    if bypassed.any?
      details = bypassed.map { |cop, count| "#{cop}: #{count}" }.join(", ")
      puts "\noxcop: #{bypassed.values.sum} examples passed without invoking oxcop, so they verify " \
           "nothing about it (#{details})."
    end

    examples = RSpec.world.example_groups.flat_map(&:descendants).flat_map(&:examples)
    unported = examples.select { |example| example.metadata[:unported] }
    next if unported.empty?

    cops = unported.map { |example| example.metadata[:described_class].cop_name }.uniq
    puts "\noxcop: #{Oxcop::Spec::Binary.ported_cops.size} cops ported; skipped #{unported.size} examples " \
         "for #{cops.size} unported cops (OXCOP_RUN_UNPORTED=1 runs them)."
  end
end
