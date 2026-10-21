# frozen_string_literal: true

require_relative "oxtree/version"

# Locates and runs the native `oxtree` binary. Platform gems ship it under
# `exe/<platform>/oxtree` (the layout tailwindcss-ruby uses); the plain `ruby`
# platform gem has no binary and explains why at run time.
module Oxtree
  GEM_NAME = "oxtree"
  EXE_DIR = File.expand_path("../exe", __dir__)
  REPO_ROOT = File.expand_path("../../..", __dir__)

  class ExecutableNotFound < StandardError; end

  class << self
    def executable
      return ENV["OXTREE_BINARY"] if ENV["OXTREE_BINARY"] && !ENV["OXTREE_BINARY"].empty?

      packaged_executable || development_executable || raise(ExecutableNotFound, not_found_message)
    end

    # Every argument goes straight to the binary.
    def exec(argv)
      Kernel.exec(executable, *argv)
    end

    private

    def packaged_executable
      Dir.glob(File.join(EXE_DIR, "*", "oxtree")).find do |path|
        platform = Gem::Platform.new(File.basename(File.dirname(path)))
        Gem::Platform.match_gem?(platform, GEM_NAME)
      end
    end

    # When the gem is used straight from a checkout (`gem "oxtree", path: ...`),
    # fall back to the Cargo build output.
    def development_executable
      %w[release debug].map { |profile| File.join(REPO_ROOT, "target", profile, "oxtree") }
                       .find { |path| File.executable?(path) }
    end

    def not_found_message
      <<~MSG
        oxtree does not ship a prebuilt binary for #{Gem::Platform.local} (#{RUBY_PLATFORM}).
        Supported platforms: #{Dir.children(EXE_DIR).select { |d| File.directory?(File.join(EXE_DIR, d)) }.join(", ").then { |s| s.empty? ? "none in this gem" : s }}.
        Set OXTREE_BINARY to the path of an oxtree binary built with `cargo build --release -p oxtree`.
      MSG
    end
  end
end
