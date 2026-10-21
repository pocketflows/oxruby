# frozen_string_literal: true

# Dumps the syntax_tree AST of a Ruby file in the same textual format as
# `oxtree --dump-ast`, so the translation layer can be diffed against the
# reference parser:
#
#   ruby crates/oxtree/tools/dump_ast.rb FILE        # syntax_tree's tree
#   target/debug/oxtree --dump-ast FILE              # oxtree's tree
#
# With `-o DIR FILE...`, writes DIR/<basename>.ast for each file instead.
#
# Needs the syntax_tree gem (6.3.0) on the load path.

require "syntax_tree"

module DumpAst
  module_function

  FIELDS =
    SyntaxTree
      .constants
      .map { |name| SyntaxTree.const_get(name) }
      .select { |klass| klass.is_a?(Class) && klass < SyntaxTree::Node }
      .to_h do |klass|
        params = klass.instance_method(:initialize).parameters.map(&:last)
        [klass, params - [:location]]
      end

  def loc(node)
    l = node.location
    "#{l.start_line}:#{l.start_char}-#{l.end_line}:#{l.end_char}"
  end

  def dump(value, indent, out, label)
    pad = "  " * indent
    case value
    when SyntaxTree::Comment, SyntaxTree::EmbDoc
      flag = value.leading? ? "L" : (value.trailing? ? "T" : "-")
      out << "#{pad}#{label}#{value.class.name.split("::").last} #{loc(value)} #{flag} #{value.value.inspect}\n"
    when SyntaxTree::Node
      name = value.class.name.split("::").last
      out << "#{pad}#{label}#{name} #{loc(value)}\n"
      (value.respond_to?(:comments) ? value.comments : []).each { |comment| dump(comment, indent + 1, out, "#") }
      FIELDS.fetch(value.class).each do |field|
        next if field == :comments
        dump(value.public_send(field), indent + 1, out, ".#{field} ")
      end
    when Array
      out << "#{pad}#{label}[#{value.size}]\n"
      value.each { |item| dump(item, indent + 1, out, "") }
    else
      out << "#{pad}#{label}#{value.inspect}\n"
    end
  end
end

def dump_file(path)
  program = SyntaxTree.parse(SyntaxTree.read(path))
  return "PARSE ERROR\n" unless program

  out = +""
  DumpAst.dump(program, 0, out, "")
  out
rescue SyntaxTree::Parser::ParseError, StandardError => e
  "ERROR #{e.class}\n"
end

if ARGV[0] == "-o"
  dir = ARGV[1]
  ARGV[2..].each do |path|
    File.write(File.join(dir, "#{File.basename(path)}.ast"), dump_file(path))
  end
else
  print dump_file(ARGV.fetch(0))
end
