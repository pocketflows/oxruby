# oxcop

A drop-in replacement for [RuboCop](https://rubocop.org), rewritten in Rust
on top of the [Prism](https://github.com/ruby/prism) parser.

```ruby
# Gemfile
gem "oxcop", require: false
```

```sh
bundle exec oxcop            # same flags, config and output as `rubocop`
```

oxcop is early: see the [project README](https://github.com/pocketflows/oxruby)
for which cops are ported and what is not supported yet.
