# frozen_string_literal: true

require "json"
require "open3"
require "tempfile"
require "yaml"

module Oxcop
  module Spec
    # A long-lived `oxcop --server` (see crates/oxcop/src/server.rs).
    class Server
      attr_reader :env

      def initialize(path)
        @env = ENV.to_h
        @input, @output, @thread = Open3.popen2(path, "--server", err: File::NULL)
        @input.binmode
        @output.binmode
        at_exit { close }
      end

      def alive?
        @thread.alive?
      end

      # [stdout, stderr, exit status], or nil when the server has died.
      def call(args, stdin)
        request = +"#{args.size}\n"
        [*args, Dir.pwd, stdin].each { |field| request << field_bytes(field) }
        @input.write(request)
        @input.flush
        status = @output.gets or return close
        [read_field, read_field, Integer(status)]
      rescue SystemCallError, IOError, ArgumentError
        close
      end

      def close
        @input.close unless @input.closed?
        @output.close unless @output.closed?
        nil
      end

      private

      def field_bytes(string)
        bytes = string.to_s.b
        "#{bytes.bytesize}\n".b << bytes
      end

      def read_field
        length = Integer(@output.gets || raise(IOError, "oxcop --server closed its output"))
        bytes = @output.read(length)
        raise IOError, "oxcop --server closed its output" unless bytes&.bytesize == length

        bytes.force_encoding(Encoding::UTF_8)
      end
    end

    # Thin wrapper around the oxcop binary for the spec shim.
    module Binary
      ROOT = File.expand_path("../..", __dir__)

      class Error < StandardError; end

      class << self
        # Investigations sent to the binary; lets the spec helper spot examples
        # that never reach oxcop (e.g. ones driving RuboCop's Commissioner).
        attr_reader :investigations

        def path
          @path ||= ENV.fetch("OXCOP_BINARY") { File.join(ROOT, "target", "debug", "oxcop") }
        end

        def ported?(cop_name)
          ported_cops.include?(cop_name)
        end

        def ported_cops
          @ported_cops ||= run("--show-ported-cops").first.lines(chomp: true).to_set
        end

        # Offenses for `source` as RuboCop's JSON formatter would report them.
        # `path` is the buffer name. `priors` are the `[name, source]` pairs
        # the example investigated before with the same cop object, which the
        # binary replays first so the cop's state carries over. With
        # `real_file`, the binary inspects `path` on disk (where the spec
        # wrote `source`) instead of reading `--stdin`.
        # With `autocorrect` (the cop's `@options[:autocorrect]`, which
        # `expect_offense` sets), the cop investigates as it would when
        # correcting (`autocorrect?` is true), but only once and without
        # keeping any correction.
        def offenses(source, path:, config:, cop_name:, priors: [], real_file: false, offenses_to_check: nil,
                     warnings: true, autocorrect: false, project_index: nil)
          @investigations = investigations.to_i + 1
          correct = autocorrect ? ["-A", "--autocorrect-once"] : []
          stdout, = invoke(source, path: path, config: config, cop_name: cop_name, priors: priors,
                                   real_file: real_file, offenses_to_check: offenses_to_check,
                                   warnings: warnings, project_index: project_index,
                                   args: [*correct, "--format", "json", "--no-display-cop-names"])
          # A correction written to the real file is undone: the spec's file
          # keeps the source, as an in-memory correction would (a cop's side
          # effects, like Lint/ScriptPermission's chmod, stay).
          File.binwrite(path, source) if real_file && autocorrect && File.binread(path) != source.b
          JSON.parse(stdout).fetch("files").flat_map { |file| file["offenses"] }
        end

        # `source` after autocorrecting with every correction (`-A`) until no
        # more changes are made, in the source's encoding (so it compares
        # equal to a spec's binary expected correction).
        def autocorrect(source, path:, config:, cop_name:, once: false, priors: [], real_file: false,
                        offenses_to_check: nil, project_index: nil)
          stdout, = invoke(source, path: path, config: config, cop_name: cop_name, priors: priors,
                                   real_file: real_file, offenses_to_check: offenses_to_check,
                                   project_index: project_index,
                                   args: ["--persist-cops", "--quiet-first-pass",
                                          *("--autocorrect-once" if once), "-A",
                                          "--format", "quiet", "--stderr"])
          return stdout.force_encoding(source.encoding) unless real_file

          # The correction was written to the file; the spec's file keeps
          # the source, as an in-memory correction would.
          File.binread(path).force_encoding(source.encoding).tap { File.binwrite(path, source) }
        end

        # `offenses_to_check`: Lint/RedundantCopDisableDirective's, as
        # `[{"cop_name", "line", "last_line"}]`. `project_index`: the
        # `[uri, source]` pairs of the cop's `project_index`
        # (spec/oxcop/project_index.rb) and the stubbed lockfile gems
        # (`gem_versions_in_target`, `path_sourced_gems_in_target`).
        def invoke(source, path:, config:, cop_name:, priors:, real_file:, args:, offenses_to_check: nil,
                   warnings: true, project_index: nil)
          input = real_file ? [path] : ["--stdin", path, "--keep-stdin-path"]
          if project_index
            files = [JSON.generate(project_index.fetch(:sources)), JSON.generate(project_index.fetch(:target_gems))]
            return with_temp_files(files) do |(index_path, gems_path)|
              invoke(source, path: path, config: config, cop_name: cop_name, priors: priors,
                             real_file: real_file,
                             args: [*args, "--project-index-sources", index_path, "--target-gems", gems_path],
                             offenses_to_check: offenses_to_check, warnings: warnings)
            end
          end
          if offenses_to_check
            return with_temp_files([JSON.generate(offenses_to_check)]) do |(offenses_path)|
              invoke(source, path: path, config: config, cop_name: cop_name, priors: priors,
                             real_file: real_file, args: [*args, "--offenses-to-check", offenses_path],
                             warnings: warnings)
            end
          end

          with_config(config) do |config_path|
            with_temp_files(priors.map(&:last)) do |prior_paths|
              prior_args = priors.zip(prior_paths).flat_map { |(name, _), file| ["--prior", "#{name}=#{file}"] }
              with_temp_files([JSON.generate(auto_gen_state(cop_name))]) do |(state_path)|
                with_report(warnings: warnings) do |report_path|
                  run(*input, "-c", config_path, "--raw-config", "--only", cop_name, *prior_args,
                      "--auto-gen-state", state_path, *("--lsp" if lsp?), *default_external_args,
                      "--spec-report", report_path,
                      *args, stdin: real_file ? "" : source)
                end
              end
            end
          end
        end

        # The binary's `--spec-report`: re-raises what a cop raised (RuboCop's
        # specs run cops with `raise_error: true`) and copies the
        # `--auto-gen-config` state into Ruby, where the specs read it
        # (`cop.config_to_allow_offenses`, `read_exclude_limit`). The binary
        # starts from the current state (`--auto-gen-state`), so it
        # accumulates across calls as RuboCop's class-level hashes do.
        # Warnings the binary printed (`Kernel#warn` upstream) go to the
        # spec's `$stderr`, unless this call repeats an investigation.
        def with_report(warnings: true)
          path = File.join(Dir.tmpdir, "oxcop-spec-report-#{Process.pid}.json")
          File.delete(path) if File.exist?(path)
          begin
            result = yield path
          rescue Error => e
            failed = e
          end
          report = File.exist?(path) ? JSON.parse(File.read(path)) : {}
          if (error = report.fetch("raised", []).first)
            raise error_class(error.fetch("class")), error.fetch("message")
          end
          raise failed if failed

          apply_auto_gen_state(report)
          report.fetch("warnings", []).each { |warning| $stderr.puts(warning) } if warnings
          result
        end

        private

        def error_class(name)
          klass = Object.const_get(name)
          klass.is_a?(Class) && klass <= Exception ? klass : RuntimeError
        rescue NameError
          RuntimeError
        end

        def apply_auto_gen_state(report)
          formatter = RuboCop::Formatter::DisabledConfigFormatter
          report.fetch("config_to_allow_offenses", {}).each do |cop, config|
            formatter.config_to_allow_offenses[cop] = config
          end
          report.fetch("detected_styles", {}).each do |cop, styles|
            formatter.detected_styles[cop] = styles
          end
          report.fetch("exclude_limits", {}).each do |cop, limits|
            cop_dir = RuboCop::ExcludeLimit.cop_dir_for(cop) or next
            limits.each do |parameter, value|
              cop_dir.mkpath
              cop_dir.join(parameter).write("#{value}\n", mode: "a")
            end
          end
        end

        # `String#inspect` (which some corrections use) escapes for this
        # encoding, which a spec may change (`with_default_external_encoding`).
        def default_external_args
          encoding = Encoding.default_internal || Encoding.default_external
          encoding == Encoding::UTF_8 ? [] : ["--default-external", encoding.name]
        end

        def lsp?
          defined?(RuboCop::LSP) && RuboCop::LSP.enabled?
        end

        def auto_gen_state(cop_name)
          formatter = RuboCop::Formatter::DisabledConfigFormatter
          state = { "config_to_allow_offenses" => {}, "detected_styles" => {} }
          if (config = formatter.config_to_allow_offenses&.[](cop_name))
            state["config_to_allow_offenses"][cop_name] = config
          end
          if (styles = formatter.detected_styles&.[](cop_name))
            state["detected_styles"][cop_name] = styles.map(&:to_s)
          end
          state
        end

        def with_temp_files(contents, paths = [], &block)
          return yield paths if contents.empty?

          Tempfile.create("oxcop-spec") do |file|
            file.binmode
            file.write(contents.first)
            file.flush
            with_temp_files(contents.drop(1), paths + [file.path], &block)
          end
        end

        # Strings that aren't valid in their encoding (`"\x82"` in a cop's
        # config) can't be dumped as text; as binary strings Psych writes
        # them as `!binary`, which the binary decodes.
        def dumpable(value)
          case value
          when Hash then value.to_h { |key, val| [dumpable(key), dumpable(val)] }
          when Array then value.map { |element| dumpable(element) }
          when String then value.valid_encoding? ? value : value.b
          else value
          end
        end

        def with_config(config)
          Tempfile.create(["oxcop-spec", ".yml"]) do |file|
            file.write(YAML.dump(dumpable(config.to_h)))
            file.flush
            yield file.path
          end
        end

        def run(*args, stdin: "")
          unless File.executable?(path)
            raise Error, "oxcop binary not found at #{path}; run `cargo build` or set OXCOP_BINARY"
          end

          stdout, stderr, status = server_run(args, stdin) || spawn_run(args, stdin)
          raise Error, "oxcop #{args.join(' ')} exited #{status}:\n#{stderr}" if status > 1

          [stdout, stderr]
        end

        def spawn_run(args, stdin)
          stdout, stderr, status = Open3.capture3(path, *args, stdin_data: stdin)
          [stdout, stderr, status.exitstatus || 128 + status.termsig.to_i]
        end

        # Spawning a process from the spec process is slow once every spec
        # file is loaded (its heap is large), so requests go to one
        # `oxcop --server` process instead. Anything the server can't answer
        # the same way a fresh process would (a changed ENV, a crash) runs
        # in a process of its own. OXCOP_NO_SERVER=1 always spawns.
        def server_run(args, stdin)
          return if ENV["OXCOP_NO_SERVER"]

          @server = nil if @server && !@server.alive?
          @server ||= Server.new(path)
          return unless @server.env == ENV.to_h

          @server.call(args, stdin) || (@server = nil)
        end
      end
    end
  end
end
