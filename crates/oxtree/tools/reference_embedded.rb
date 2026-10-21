# frozen_string_literal: true

# Like reference_ruby.rb, for the app's ErbEmbeddedFormatter with OXFMT_BIN pointed at
# an oxfmt 0.72.0 installed outside code/ (read-only on ROOT):
#   ruby reference_embedded.rb OXFMT_BIN ROOT OUT_DIR WIDTH LIST
# Compare: oxtree --format-embedded-list ROOT OUT WIDTH LIST library|process OXFMT_ROOT OXFMT_BIN

require "syntax_tree"
require "syntax_tree/erb"
oxfmt, root, out, width, list = ARGV
load File.join(root, "lib/tasks/support/erb_embedded_formatter.rb")
ErbEmbeddedFormatter.send(:remove_const, :OXFMT_BIN)
ErbEmbeddedFormatter.const_set(:OXFMT_BIN, oxfmt)
File.readlines(list, chomp: true).each do |rel|
  path = File.join(root, rel)
  src = ErbEmbeddedFormatter.read(path)
  res = begin
    ErbEmbeddedFormatter.format(src, width.to_i)
  rescue StandardError => e
    "ERROR #{e.class}: #{e.message.lines.first&.strip}"
  end
  File.write(File.join(out, rel.gsub("/", "__")), res)
end
