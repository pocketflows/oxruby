# Used by script/compare_regexp_parser.
# Collects regexp literal sources (interpolations blanked, as RuboCop's
# with_interpolations_blanked does) from Ruby files given on ARGV.
require "prism"
require "json"
seen = {}
ARGV.each do |path|
  begin
    src = File.read(path)
    result = Prism.parse(src)
  rescue
    next
  end
  queue = [result.value]
  until queue.empty?
    node = queue.pop
    next unless node
    case node
    when Prism::RegularExpressionNode, Prism::InterpolatedRegularExpressionNode
      parts = node.is_a?(Prism::RegularExpressionNode) ? [node.content_loc.slice] :
        node.parts.map { |p| p.is_a?(Prism::StringNode) ? p.location.slice : " " * p.location.slice.length }
      body = parts.join
      opts = node.closing_loc.slice[1..].to_s
      key = [body, opts.include?("x"), opts.include?("n")]
      seen[key] = true
    end
    if node.is_a?(Prism::StringNode) && node.unescaped.include?("/") && node.unescaped.size < 5000
      inner = (Prism.parse(node.unescaped).value rescue nil)
      queue << inner if inner
    end
    queue.concat(node.compact_child_nodes)
  end
end
seen.each_key { |k| next unless k[0].dup.force_encoding("UTF-8").valid_encoding?; k[0] = k[0].dup.force_encoding("UTF-8"); puts JSON.generate(k) }
