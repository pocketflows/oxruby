# frozen_string_literal: true

require "fileutils"
require "rubygems/package"
require "rspec/core/rake_task"

ROOT = __dir__
GEM_DIR = File.join(ROOT, "gems", "oxcop")
PKG_DIR = File.join(ROOT, "pkg")

# RubyGems platform => Rust target triple for the native gems we publish.
NATIVE_PLATFORMS = {
  "arm64-darwin" => "aarch64-apple-darwin",
  "x86_64-darwin" => "x86_64-apple-darwin",
  "x86_64-linux-gnu" => "x86_64-unknown-linux-gnu",
  "aarch64-linux-gnu" => "aarch64-unknown-linux-gnu",
  "x86_64-linux-musl" => "x86_64-unknown-linux-musl",
  "aarch64-linux-musl" => "aarch64-unknown-linux-musl"
}.freeze

def cargo_version
  File.read(File.join(ROOT, "Cargo.toml"))[/^\[workspace\.package\].*?^version = "([^"]+)"/m, 1]
end

def gem_version
  File.read(File.join(GEM_DIR, "lib", "oxcop", "version.rb"))[/VERSION = "([^"]+)"/, 1]
end

def build_gem(platform: nil)
  spec = Dir.chdir(GEM_DIR) { Gem::Specification.load("oxcop.gemspec") }
  spec.platform = platform if platform
  FileUtils.mkdir_p(PKG_DIR)
  Dir.chdir(GEM_DIR) do
    file = Gem::Package.build(spec)
    FileUtils.mv(file, PKG_DIR)
    puts "built pkg/#{file}"
  end
end

desc "Build the oxcop binary (debug)"
task :build do
  sh "cargo build"
end

namespace :cargo do
  desc "cargo fmt --check, clippy and test"
  task :check do
    sh "cargo fmt --all --check"
    sh "cargo clippy --all-targets -- -D warnings"
    sh "cargo test"
  end
end

desc "Run RuboCop's vendored cop specs against the oxcop binary"
RSpec::Core::RakeTask.new(:spec)
task spec: :build

desc "Diff oxcop against real RuboCop on ../code (or DIR=...)"
task :compare do
  sh "cargo build --release"
  sh "script/compare_with_rubocop", *[ENV["DIR"]].compact
end

namespace :version do
  desc "Check that Cargo.toml and the gem agree on the version"
  task :check do
    next if cargo_version == gem_version

    abort "version mismatch: Cargo.toml has #{cargo_version}, gems/oxcop has #{gem_version}"
  end
end

namespace :gem do
  desc "Build the plain `ruby` platform gem (no binary; explains how to get one)"
  task ruby: "version:check" do
    build_gem
  end

  # A native gem for PLATFORM. The binary comes from BINARY=path when given
  # (CI cross-builds it), otherwise from `cargo build --release --target`.
  desc "Build a native gem, e.g. rake gem:native[arm64-darwin]"
  task :native, [:platform] => "version:check" do |_, args|
    platform = args.fetch(:platform)
    target = NATIVE_PLATFORMS.fetch(platform) { abort "unknown platform #{platform}" }
    binary = ENV["BINARY"] || begin
      sh "cargo build --release --target #{target}"
      File.join(ROOT, "target", target, "release", "oxcop")
    end

    exe_dir = File.join(GEM_DIR, "exe", platform)
    FileUtils.mkdir_p(exe_dir)
    FileUtils.cp(binary, File.join(exe_dir, "oxcop"))
    FileUtils.chmod(0o755, File.join(exe_dir, "oxcop"))
    begin
      build_gem(platform: platform)
    ensure
      FileUtils.rm_rf(exe_dir)
    end
  end
end

namespace :vendor do
  desc "Re-sync vendor/rubocop and the cop specs from the ../rubocop checkout"
  task :rubocop do
    upstream = File.expand_path("../rubocop", ROOT)
    tag = `git -C #{upstream} describe --tags`.strip
    FileUtils.cp(File.join(upstream, "config", "default.yml"), File.join(ROOT, "vendor/rubocop/config/default.yml"))
    FileUtils.cp(File.join(upstream, "LICENSE.txt"), File.join(ROOT, "vendor/rubocop/LICENSE.txt"))
    %w[bundler gemspec layout lint metrics migration naming security style].each do |department|
      FileUtils.rm_rf(File.join(ROOT, "spec/rubocop/cop", department))
      FileUtils.cp_r(File.join(upstream, "spec/rubocop/cop", department), File.join(ROOT, "spec/rubocop/cop"))
    end
    # Unit tests of RuboCop's Ruby helpers rather than of cop behavior.
    FileUtils.rm_rf(File.join(ROOT, "spec/rubocop/cop/metrics/utils"))
    puts "synced from rubocop #{tag}; update RUBOCOP_VERSION in version.rs/version.rb and the Gemfile to match"
  end
end

task default: %w[version:check cargo:check spec]
