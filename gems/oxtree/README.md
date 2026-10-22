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
bundle exec oxtree .                 # format the project's Ruby files in place
bundle exec oxtree --check app lib   # exit 1 if any file isn't formatted
bundle exec oxtree --list-different . --ignore-files=db --ignore-files=vendor
bundle exec oxtree check '**/*.rb'   # stree's commands work too
```

Options and `.streerc` are syntax_tree's (`--print-width`, `--plugins`,
`--ignore-files`, `--config`); the print width defaults to 100. See
`oxtree --help`.

The gem installs a single `oxtree` executable. It has no `stree` alias, so
it never shadows syntax_tree's when both are bundled.

oxtree is early: see the [project README](https://github.com/theodoretliu/oxruby)
for status.
