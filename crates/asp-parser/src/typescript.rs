use anyhow::Result;
use tree_sitter::Parser;
use crate::{Language, LanguageParser, ParseResult, RawImport, Symbol, SymbolKind};

pub struct TypeScriptParser;

impl LanguageParser for TypeScriptParser {
    fn parse(&self, source: &[u8]) -> Result<ParseResult> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&lang)?;

        let tree = parser.parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse TypeScript source"))?;

        let source_str = std::str::from_utf8(source).unwrap_or("");
        let mut imports = Vec::new();
        let mut symbols = Vec::new();

        collect_nodes(tree.root_node(), source_str, &mut imports, &mut symbols);

        Ok(ParseResult {
            imports,
            symbols,
            language: Language::TypeScript,
        })
    }

    fn language(&self) -> Language {
        Language::TypeScript
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
            let line = node.start_position().row + 1;
            let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            imports.push(RawImport { source: text, line });
        }
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Function,
                    line: node.start_position().row + 1,
                    exported: false,
                });
            }
        }
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                symbols.push(Symbol {
                    name,
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
