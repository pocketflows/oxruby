# frozen_string_literal: true

source "https://rubygems.org"

gem "rake", "~> 13.0"
gem "rspec", "~> 3.13"

# The reference implementation. The vendored specs under spec/rubocop/ load it
# for cop constants, the `:config` shared context and `expect_offense`; the
# offenses themselves come from the oxcop binary (see spec/oxcop/).
gem "rubocop", "1.91.0"

# Plugins whose cops oxcop ports (spec/rubocop/cop/rails/, spec/rubocop/cop/rspec/),
# pinned to the versions ../code uses. Their vendored specs load them the same way.
gem "rubocop-rails", "2.37.0"
gem "rubocop-rspec", "3.10.2"
