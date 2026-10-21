# frozen_string_literal: true

require "json"
require "open3"
require "tempfile"
require "yaml"

module Oxcop
  module Spec
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
            stdout, = run("--stdin", path, "-c", config_path, "--raw-config", "--only", cop_name,
                          "--format", "json", "--no-display-cop-names", stdin: source)
            JSON.parse(stdout).fetch("files").flat_map { |file| file["offenses"] }
          end
        end

        # `source` after autocorrecting with every correction (`-A`) until no
        # more changes are made.
        def autocorrect(source, path:, config:, cop_name:, once: false)
          with_config(config) do |config_path|
            stdout, = run("--stdin", path, "-c", config_path, "--raw-config", "--persist-cops",
                          *("--autocorrect-once" if once),
                          "--only", cop_name, "-A", "--format", "quiet", "--stderr", stdin: source)
            stdout
          end
        end

        private

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

          stdout, stderr, status = Open3.capture3(path, *args, stdin_data: stdin)
          raise Error, "oxcop #{args.join(' ')} exited #{status.exitstatus}:\n#{stderr}" if status.exitstatus > 1

          [stdout, stderr]
        end
      end
    end
  end
end
