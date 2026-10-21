# oxtree

A drop-in replacement for [syntax_tree](https://github.com/ruby-syntax-tree/syntax_tree)
and its ERB plugin ([w_syntax_tree-erb](https://github.com/davidwessman/syntax_tree-erb)),
rewritten in Rust on top of the [Prism](https://github.com/ruby/prism) parser.
Its output is byte-for-byte syntax_tree's.

```ruby
# Gemfile
gem "oxtree", require: false
```

```sh
bundle exec oxtree check '**/*.rb'   # stree's commands, options and output
```

The gem installs only an `oxtree` executable. An `stree`-named alias, which
would shadow syntax_tree's, is included only in gems built with
`OXTREE_STREE=1`.

oxtree is early: see the [project README](https://github.com/theodoretliu/oxruby)
for status.
