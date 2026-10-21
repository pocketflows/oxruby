# Used by script/compare_regexp_parser.
require "regexp_parser"
require "json"
def dump(exp, depth, out)
  q = exp.quantifier
  out << [depth, exp.type, exp.token, exp.text, exp.ts, exp.full_length,
          q && [q.text, q.min, q.max, q.mode]].to_json
  exp.expressions.each { |e| dump(e, depth + 1, out) } unless exp.terminal?
end
$stdin.each_line do |line|
  body, x, n = JSON.parse(line)
  opts = (x ? Regexp::EXTENDED : 0) | (n ? Regexp::NOENCODING : 0)
  out = []
  begin
    dump(Regexp::Parser.parse(body, options: opts), 0, out)
    puts out.join(" ")
  rescue StandardError => e
    puts "ERR"
  end
end
