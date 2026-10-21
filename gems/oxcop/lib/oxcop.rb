# frozen_string_literal: true

require_relative "oxcop/version"

# Locates and runs the native `oxcop` binary. Platform gems ship it under
# `exe/<platform>/oxcop` (the layout tailwindcss-ruby uses); the plain `ruby`
# platform gem has no binary and explains why at run time.
module Oxcop
  GEM_NAME = "oxcop"
  EXE_DIR = File.expand_path("../exe", __dir__)
  REPO_ROOT = File.expand_path("../../..", __dir__)

  class ExecutableNotFound < StandardError; end

  class << self
    def executable
      return ENV["OXCOP_BINARY"] if ENV["OXCOP_BINARY"] && !ENV["OXCOP_BINARY"].empty?

      packaged_executable || development_executable || raise(ExecutableNotFound, not_found_message)
    end

    def exec(argv)
      Kernel.exec(ruby_env, executable, *argv)
    end

    private

    def packaged_executable
      Dir.glob(File.join(EXE_DIR, "*", "oxcop")).find do |path|
        platform = Gem::Platform.new(File.basename(File.dirname(path)))
        Gem::Platform.match_gem?(platform, GEM_NAME)
      end
    end

    # When the gem is used straight from a checkout (`gem "oxcop", path: ...`),
    # fall back to the Cargo build output.
    def development_executable
      %w[release debug].map { |profile| File.join(REPO_ROOT, "target", profile, "oxcop") }
                       .find { |path| File.executable?(path) }
    end

    def ruby_env
      {
        "OXCOP_RUBY_ENGINE" => RUBY_ENGINE,
        "OXCOP_RUBY_VERSION" => RUBY_VERSION,
        "OXCOP_RUBY_PATCHLEVEL" => RUBY_PATCHLEVEL.to_s,
        "OXCOP_RUBY_PLATFORM" => RUBY_PLATFORM,
        "OXCOP_GEMS" => gems
      }
    end

    # The gems RuboCop could load plugins (and `inherit_gem` configs) from,
    # one `name<TAB>full_gem_path<TAB>default_lint_roller_plugin` per line:
    # the bundle's under Bundler, otherwise the latest installed versions.
    def gems
      specs = defined?(Bundler) && !Gem.loaded_specs.empty? ? Gem.loaded_specs.values : Gem::Specification.latest_specs(true)
      specs.map { |spec| [spec.name, spec.full_gem_path, spec.metadata["default_lint_roller_plugin"]].join("\t") }
           .join("\n")
    end

    def not_found_message
      <<~MSG
        oxcop does not ship a prebuilt binary for #{Gem::Platform.local} (#{RUBY_PLATFORM}).
        Supported platforms: #{Dir.children(EXE_DIR).select { |d| File.directory?(File.join(EXE_DIR, d)) }.join(", ").then { |s| s.empty? ? "none in this gem" : s }}.
        Set OXCOP_BINARY to the path of an oxcop binary built with `cargo build --release`.
      MSG
    end
  end
end
