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
        def offenses(source, path:, config:, cop_name:)
          @investigations = investigations.to_i + 1
          with_config(config) do |config_path|
            with_report do |report_path|
              stdout, = run("--stdin", path, "-c", config_path, "--raw-config", "--only", cop_name,
                            "--format", "json", "--no-display-cop-names", "--spec-report", report_path,
                            stdin: source)
              JSON.parse(stdout).fetch("files").flat_map { |file| file["offenses"] }
            end
          end
        end

        # `source` after autocorrecting with every correction (`-A`) until no
        # more changes are made.
        def autocorrect(source, path:, config:, cop_name:, once: false)
          with_config(config) do |config_path|
            with_report(apply: false) do |report_path|
              stdout, = run("--stdin", path, "-c", config_path, "--raw-config", "--persist-cops",
                            *("--autocorrect-once" if once),
                            "--only", cop_name, "-A", "--format", "quiet", "--stderr",
                            "--spec-report", report_path, stdin: source)
              stdout
            end
          end
        end

        # The binary's `--spec-report`: re-raises what a cop raised (RuboCop's
        # specs run cops with `raise_error: true`) and, with `apply`, copies
        # the `--auto-gen-config` state into Ruby, where the specs read it
        # (`cop.config_to_allow_offenses`, `read_exclude_limit`).
        def with_report(apply: true)
          path = File.join(Dir.tmpdir, "oxcop-spec-report-#{Process.pid}.json")
          File.delete(path) if File.exist?(path)
          result = yield path
          report = File.exist?(path) ? JSON.parse(File.read(path)) : {}
          if (error = report.fetch("raised", []).first)
            raise error_class(error.fetch("class")), error.fetch("message")
          end
          apply_auto_gen_state(report) if apply
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

        def with_config(config)
          Tempfile.create(["oxcop-spec", ".yml"]) do |file|
            file.write(YAML.dump(config.to_h))
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
