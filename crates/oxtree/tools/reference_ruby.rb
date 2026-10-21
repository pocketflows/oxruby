# frozen_string_literal: true

# Reference outputs from real syntax_tree for differential testing:
#   ruby reference_ruby.rb ROOT OUT_DIR WIDTH LIST
# formats every path in LIST (relative to ROOT) and writes OUT_DIR/<path with / as __>.
# Compare with `oxtree --format-list ROOT OUT_DIR WIDTH LIST` (a dev-only command).

require "syntax_tree"
root, out, width, list = ARGV
File.readlines(list, chomp: true).each do |rel|
  src = SyntaxTree.read(File.join(root, rel))
  res = begin
    SyntaxTree.format(src, width.to_i)
  rescue => e
    "ERROR #{e.class}: #{e.message.lines.first}"
  end
  dest = File.join(out, rel.gsub("/", "__"))
  File.write(dest, res)
end
