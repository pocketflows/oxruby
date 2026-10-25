# frozen_string_literal: true

require_relative "lib/oxcop/version"

Gem::Specification.new do |spec|
  spec.name = "oxcop"
  spec.version = Oxcop::VERSION
  spec.authors = ["Teddy Liu"]
  spec.email = ["theodoretliu@gmail.com"]

  spec.summary = "A drop-in replacement for RuboCop, rewritten in Rust on the Prism parser"
  spec.description = <<~DESC
    oxcop reads your .rubocop.yml, accepts RuboCop's command-line options and
    prints RuboCop-compatible output, but runs as a native binary. It tracks
    RuboCop #{Oxcop::RUBOCOP_VERSION}.
  DESC
  spec.homepage = "https://github.com/pocketflows/oxruby"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.1"

  spec.metadata = {
    "homepage_uri" => spec.homepage,
    "source_code_uri" => spec.homepage,
    "rubygems_mfa_required" => "true"
  }

  spec.files = Dir["lib/**/*.rb", "exe/oxcop", "exe/*/oxcop", "LICENSE.txt", "THIRD_PARTY_NOTICES.md", "README.md"]
  spec.bindir = "exe"
  spec.executables = ["oxcop"]
end
