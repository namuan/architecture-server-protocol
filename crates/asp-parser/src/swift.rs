use anyhow::Result;
use tree_sitter::Parser;
use crate::{Language, LanguageParser, ParseResult, RawImport, Symbol, SymbolKind};

pub struct SwiftParser;

impl LanguageParser for SwiftParser {
    fn parse(&self, source: &[u8]) -> Result<ParseResult> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_swift::LANGUAGE.into();
        parser.set_language(&lang)?;

        let tree = parser.parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse Swift source"))?;

        let source_str = std::str::from_utf8(source).unwrap_or("");
        let mut imports = Vec::new();
        let mut symbols = Vec::new();

        collect_nodes(tree.root_node(), source_str, &mut imports, &mut symbols);

        Ok(ParseResult { imports, symbols, language: Language::Swift })
    }

    fn language(&self) -> Language {
        Language::Swift
    }
}

fn collect_nodes(
    node: tree_sitter::Node,
    source: &str,
    imports: &mut Vec<RawImport>,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "import_declaration" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            imports.push(RawImport { source: text, line: node.start_position().row + 1 });
        }
        "function_declaration" | "init_declaration" | "deinit_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: name.utf8_text(source.as_bytes()).unwrap_or("").to_string(),
                    kind: SymbolKind::Function,
                    line: node.start_position().row + 1,
                    exported: false,
                });
            }
        }
        "class_declaration" | "struct_declaration" | "protocol_declaration" | "enum_declaration" => {
            if let Some(name) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: name.utf8_text(source.as_bytes()).unwrap_or("").to_string(),
                    kind: SymbolKind::Class,
                    line: node.start_position().row + 1,
                    exported: false,
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_nodes(child, source, imports, symbols);
    }
}
