# frozen_string_literal: true

# Like reference_ruby.rb, for SyntaxTree::ERB.format (compare: oxtree --format-erb-list).

require "syntax_tree"
require "syntax_tree/erb"
root, out, width, list = ARGV
File.readlines(list, chomp: true).each do |rel|
  src = File.read(File.join(root, rel))
  res = begin
    SyntaxTree::ERB.format(src, width.to_i)
  rescue SyntaxTree::Parser::ParseError => e
    "ERROR #{e.message} #{e.lineno}:#{e.column}"
  rescue => e
    "ERROR #{e.class}: #{e.message.lines.first}"
  end
  File.write(File.join(out, rel.gsub("/", "__")), res)
end
