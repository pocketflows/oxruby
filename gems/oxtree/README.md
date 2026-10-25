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
bundle exec oxtree .                 # format the project's Ruby and ERB files in place
bundle exec oxtree --check app lib   # exit 1 if any file isn't formatted
bundle exec oxtree --list-different . --ignore-files=db --ignore-files=vendor
bundle exec oxtree check '**/*.rb'   # stree's commands work too
```

Options and `.streerc` are syntax_tree's (`--print-width`, `--plugins`,
`--ignore-files`, `--config`); the print width defaults to 100. See
`oxtree --help`.

A directory also covers its `*.html.erb`, `*.js.erb`, `*.css.erb` and
`*.json.erb` files, with no flag. They are formatted with syntax_tree's ERB
plugin, except that `<script>`/`<style>` bodies and whole `.js.erb`,
`.css.erb` and `.json.erb` files go through oxfmt (built in; the project's
`.oxfmtrc.json` applies). The `oxfmt` executable (`node_modules/.bin/oxfmt`
or `OXFMT_BIN`) is only needed for Tailwind class sorting and a few rare
inputs; without it, Tailwind classes are left unsorted with a warning. stree's own commands (`oxtree check FILE`)
keep syntax_tree's ERB handling, behind `--plugins=erb`.

The gem installs a single `oxtree` executable. It has no `stree` alias, so
it never shadows syntax_tree's when both are bundled.

oxtree is early: see the [project README](https://github.com/pocketflows/oxruby)
for status.
