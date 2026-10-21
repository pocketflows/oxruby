# frozen_string_literal: true

module Oxcop
  module Spec
    # Replaces `CopHelper#_investigate` (lib/rubocop/rspec/cop_helper.rb) so
    # that `expect_offense`, `expect_no_offenses`, `expect_correction` and
    # friends assert on oxcop's results instead of the Ruby cop's.
    module CopHelper
      # Stands in for the `RuboCop::Cop::Corrector` the upstream helpers
      # inspect after an investigation. oxcop is only asked to correct when a
      # spec actually looks at the correction.
      class Corrector
        def initialize(source, &autocorrect)
          @source = source
          @autocorrect = autocorrect
        end

        def rewrite
          @rewrite ||= @autocorrect.call
        end

        def empty?
          rewrite == @source
        end
      end

      # `expect_correction(loop: false)` checks a single correction pass.
      def expect_correction(correction, loop: true, source: nil)
        @oxcop_single_pass = !loop
        super
      ensure
        @oxcop_single_pass = false
      end

      def _investigate(cop, processed_source)
        source = processed_source.raw_source
        options = {
          path: processed_source.buffer.name,
          config: cop.config,
          cop_name: cop.cop_name,
          # The cop object keeps its state across the example's
          # investigations, so the binary replays the earlier ones.
          priors: (@oxcop_priors ||= []).dup,
          real_file: real_file?(cop, processed_source),
          offenses_to_check: offenses_to_check(cop)
        }
        cop_options = cop.instance_variable_get(:@options) || {}

        # oxcop runs the whole correction loop itself (with `--persist-cops`, as
        # the `expect_correction` loop keeps the same cop instance). When
        # `expect_correction` re-investigates oxcop's final source, the loop
        # already reached its fixed point there, so there is nothing left to
        # correct. A cop told not to autocorrect makes no corrections.
        @oxcop_corrected_sources ||= []
        @last_corrector = if @oxcop_corrected_sources.include?(source) || cop_options[:autocorrect] == false
                            Corrector.new(source) { source }
                          else
                            Corrector.new(source) do
                              Binary.autocorrect(source, once: @oxcop_single_pass, **options)
                                    .tap { |s| @oxcop_corrected_sources << s }
                            end
                          end
        # The final source of oxcop's correction loop was investigated there.
        offenses = Binary.offenses(source, warnings: !@oxcop_corrected_sources.include?(source),
                                           autocorrect: cop_options[:autocorrect] ? true : false, **options)
        @oxcop_priors << [options[:path], source]
        offenses.map { |offense| to_offense(offense, processed_source, cop) }
      end

      private

      # Lint/RedundantCopDisableDirective is constructed with the offenses it
      # checks the directives against.
      def offenses_to_check(cop)
        return unless cop.respond_to?(:offenses_to_check) && cop.offenses_to_check

        cop.offenses_to_check.map do |offense|
          { "cop_name" => offense.cop_name, "line" => offense.line, "last_line" => offense.last_line }
        end
      end

      # The spec wrote the source to a real file (`expect_offense(source,
      # file)`) and didn't tell the cop it came from stdin: inspect the file,
      # as cops like Lint/ScriptPermission look at it on disk.
      def real_file?(cop, processed_source)
        path = processed_source.path
        return false unless path && File.file?(path)
        return false if (cop.instance_variable_get(:@options) || {}).key?(:stdin)

        File.binread(path) == processed_source.raw_source.b
      end

      def to_offense(hash, processed_source, cop)
        location = hash.fetch("location")
        buffer = processed_source.buffer
        range = if buffer.source.empty? && location.fetch("length").zero?
                  # A global offense (`add_global_offense`) on an empty file.
                  RuboCop::Cop::Offense::NO_LOCATION
                else
                  begin_pos = buffer.line_range(location.fetch("start_line")).begin_pos +
                              location.fetch("start_column") - 1
                  Parser::Source::Range.new(buffer, begin_pos, begin_pos + location.fetch("length"))
                end
        status = if hash["corrected"] then :corrected
                 elsif hash["correctable"] then :uncorrected
                 else :unsupported
                 end

        RuboCop::Cop::Offense.new(hash.fetch("severity").to_sym, range, hash.fetch("message"), cop.cop_name, status)
      end
    end
  end
end
