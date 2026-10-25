# frozen_string_literal: true

# Tasks for the oxtree crate and gem. Rake loads this after the Rakefile, so
# ROOT, PKG_DIR, NATIVE_PLATFORMS and cargo_version come from there.
# Everything lives under `oxtree:` because Rake merges same-named tasks: a
# plain `build` or `gem:native` here would also run oxcop's.

OXTREE_GEM_DIR = File.join(ROOT, "gems", "oxtree")

# Sibling checkouts that `oxtree:vendor` copies from, and what it takes.
OXTREE_VENDOR = {
  "syntax_tree" => {
    url: "https://github.com/ruby-syntax-tree/syntax_tree",
    constant: "SYNTAX_TREE_VERSION",
    paths: %w[
      test/fixtures test/plugin test/cli_test.rb test/formatting_test.rb
      test/idempotency_test.rb test/quotes_test.rb test/test_helper.rb
    ],
    readme: "(test/fixtures plus the formatting, idempotency, CLI, quotes and plugin tests)."
  },
  "syntax_tree-erb" => {
    url: "https://github.com/davidwessman/syntax_tree-erb",
    constant: "SYNTAX_TREE_ERB_VERSION",
    paths: %w[test],
    readme: "(published as the w_syntax_tree-erb gem): test/ and test/fixture/."
  }
}.freeze

def oxtree_gem_version(constant = "VERSION")
  File.read(File.join(OXTREE_GEM_DIR, "lib", "oxtree", "version.rb"))[/#{constant} = "([^"]+)"/, 1]
end

# `../name` next to the checkout. From a worktree (.worktrees/<name>) that's
# next to the main checkout instead, found through the shared git directory.
def oxtree_sibling(name)
  common = File.expand_path(`git -C #{ROOT} rev-parse --git-common-dir`.strip, ROOT)
  candidates = [File.expand_path("../#{name}", ROOT), File.expand_path("../../#{name}", common)]
  candidates.find { |dir| File.directory?(dir) } || abort("#{name}: no checkout at #{candidates.uniq.join(" or ")}")
end

def oxtree_build_gem(platform: nil)
  spec = Dir.chdir(OXTREE_GEM_DIR) { Gem::Specification.load("oxtree.gemspec") }
  spec.platform = platform if platform
  FileUtils.mkdir_p(PKG_DIR)
  Dir.chdir(OXTREE_GEM_DIR) do
    file = Gem::Package.build(spec)
    FileUtils.mv(file, PKG_DIR)
    puts "built pkg/#{file}"
  end
end

namespace :oxtree do
  desc "Build the oxtree binary (debug)"
  task :build do
    sh "cargo build -p oxtree"
  end

  desc "cargo fmt --check, clippy and test for oxtree only"
  task :check do
    sh "cargo fmt -p oxtree --check"
    sh "cargo clippy -p oxtree --all-targets -- -D warnings"
    sh "cargo test -p oxtree"
  end

  namespace :version do
    desc "Check that Cargo.toml and the oxtree gem agree on the version"
    task :check do
      next if cargo_version == oxtree_gem_version

      abort "version mismatch: Cargo.toml has #{cargo_version}, gems/oxtree has #{oxtree_gem_version}"
    end
  end

  namespace :gem do
    desc "Build the plain `ruby` platform oxtree gem (no binary; explains how to get one)"
    task ruby: %w[oxtree:version:check license:check] do
      oxtree_build_gem
    end

    # A native gem for PLATFORM. The binary comes from BINARY=path when given
    # (CI cross-builds it), otherwise from `cargo build --release --target`.
    desc "Build a native oxtree gem, e.g. rake oxtree:gem:native[arm64-darwin]"
    task :native, [:platform] => %w[oxtree:version:check license:check] do |_, args|
      platform = args.fetch(:platform)
      target = NATIVE_PLATFORMS.fetch(platform) { abort "unknown platform #{platform}" }
      binary = ENV["BINARY"] || begin
        sh "cargo build --release -p oxtree --target #{target}"
        File.join(ROOT, "target", target, "release", "oxtree")
      end

      exe_dir = File.join(OXTREE_GEM_DIR, "exe", platform)
      FileUtils.mkdir_p(exe_dir)
      FileUtils.cp(binary, File.join(exe_dir, "oxtree"))
      FileUtils.chmod(0o755, File.join(exe_dir, "oxtree"))
      begin
        oxtree_build_gem(platform: platform)
      ensure
        FileUtils.rm_rf(exe_dir)
      end
    end
  end

  desc "Re-sync vendor/syntax_tree and vendor/syntax_tree-erb from the sibling checkouts"
  task :vendor do
    OXTREE_VENDOR.each do |name, source|
      upstream = oxtree_sibling(name)
      tag = `git -C #{upstream} describe --tags`.strip
      expected = "v#{oxtree_gem_version(source[:constant])}"
      warn "#{name}: #{upstream} is at #{tag}, but #{source[:constant]} is #{expected}" if tag != expected
      missing = (source[:paths] + ["LICENSE"]).reject { |path| File.exist?(File.join(upstream, path)) }
      abort "#{name}: #{upstream} has no #{missing.join(", ")}" unless missing.empty?

      vendor = File.join(ROOT, "vendor", name)
      FileUtils.rm_rf(File.join(vendor, "test"))
      source[:paths].each do |path|
        FileUtils.mkdir_p(File.dirname(File.join(vendor, path)))
        FileUtils.cp_r(File.join(upstream, path), File.join(vendor, path))
      end
      FileUtils.cp(File.join(upstream, "LICENSE"), File.join(vendor, "LICENSE"))
      File.write(File.join(vendor, "README"), <<~README)
        Vendored from #{source[:url]} at tag #{tag}
        #{source[:readme]}
        Do not edit by hand; re-sync with `rake oxtree:vendor`.
      README
      puts "synced vendor/#{name} from #{name} #{tag}"
    end
    puts "if a tag changed, update the versions in gems/oxtree/lib/oxtree/version.rb and docs/OXTREE.md"
  end
end

# Additive: the Rakefile's default task already runs workspace-wide cargo
# fmt, clippy and test, which cover oxtree.
task default: "oxtree:version:check"
