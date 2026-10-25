# frozen_string_literal: true

require_relative "lib/oxtree/version"

Gem::Specification.new do |spec|
  spec.name = "oxtree"
  spec.version = Oxtree::VERSION
  spec.authors = ["Teddy Liu"]
  spec.email = ["theodoretliu@gmail.com"]

  spec.summary = "A drop-in replacement for syntax_tree, rewritten in Rust on the Prism parser"
  spec.description = <<~DESC
    oxtree formats Ruby and ERB exactly as syntax_tree #{Oxtree::SYNTAX_TREE_VERSION}
    and w_syntax_tree-erb #{Oxtree::SYNTAX_TREE_ERB_VERSION} do, but runs as a native
    binary.
  DESC
  spec.homepage = "https://github.com/pocketflows/oxruby"
  spec.license = "MIT"
  spec.required_ruby_version = ">= 3.1"

  spec.metadata = {
    "homepage_uri" => spec.homepage,
    "source_code_uri" => spec.homepage,
    "rubygems_mfa_required" => "true"
  }

  spec.files = Dir["lib/**/*.rb", "exe/oxtree", "exe/*/oxtree", "LICENSE.txt", "THIRD_PARTY_NOTICES.md", "README.md"]
  spec.bindir = "exe"
  spec.executables = ["oxtree"]
end
