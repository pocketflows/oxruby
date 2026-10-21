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
          path: processed_source.path || RuboCop::AST::ProcessedSource::STRING_SOURCE_NAME,
          config: cop.config,
          cop_name: cop.cop_name
        }

        # oxcop runs the whole correction loop itself (with `--persist-cops`, as
        # the `expect_correction` loop keeps the same cop instance). When
        # `expect_correction` re-investigates oxcop's final source, the loop
        # already reached its fixed point there, so there is nothing left to
        # correct.
        @oxcop_corrected_sources ||= []
        @last_corrector = if @oxcop_corrected_sources.include?(source)
                            Corrector.new(source) { source }
                          else
                            Corrector.new(source) do
                              Binary.autocorrect(source, once: @oxcop_single_pass, **options)
                                    .tap { |s| @oxcop_corrected_sources << s }
                            end
                          end
        Binary.offenses(source, **options).map { |offense| to_offense(offense, processed_source, cop) }
      end

      private

      def to_offense(hash, processed_source, cop)
        location = hash.fetch("location")
        buffer = processed_source.buffer
        begin_pos = buffer.line_range(location.fetch("start_line")).begin_pos + location.fetch("start_column") - 1
        range = Parser::Source::Range.new(buffer, begin_pos, begin_pos + location.fetch("length"))
        status = if hash["corrected"] then :corrected
                 elsif hash["correctable"] then :uncorrected
                 else :unsupported
                 end

        RuboCop::Cop::Offense.new(hash.fetch("severity").to_sym, range, hash.fetch("message"), cop.cop_name, status)
      end
    end
  end
end
