use anyhow::Result;
use tree_sitter::Parser;
use crate::{Language, LanguageParser, ParseResult, RawImport, Symbol, SymbolKind};

pub struct RustParser;

impl LanguageParser for RustParser {
    fn parse(&self, source: &[u8]) -> Result<ParseResult> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang)?;

        let tree = parser.parse(source, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse Rust source"))?;

        let source_str = std::str::from_utf8(source).unwrap_or("");
        let mut imports = Vec::new();
        let mut symbols = Vec::new();

        collect_nodes(tree.root_node(), source_str, &mut imports, &mut symbols);

        Ok(ParseResult { imports, symbols, language: Language::Rust })
    }

    fn language(&self) -> Language {
        Language::Rust
    }
}

fn collect_nodes(
    node: tree_sitter::Node,
    source: &str,
    imports: &mut Vec<RawImport>,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "use_declaration" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            imports.push(RawImport { source: text, line: node.start_position().row + 1 });
        }
        "function_item" => {
            if let Some(name) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: name.utf8_text(source.as_bytes()).unwrap_or("").to_string(),
                    kind: SymbolKind::Function,
                    line: node.start_position().row + 1,
                    exported: is_pub(node, source),
                });
            }
        }
        "struct_item" | "enum_item" | "trait_item" | "type_item" => {
            if let Some(name) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: name.utf8_text(source.as_bytes()).unwrap_or("").to_string(),
                    kind: SymbolKind::Class,
                    line: node.start_position().row + 1,
                    exported: is_pub(node, source),
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

/// Returns true if the node is preceded by a `pub` visibility modifier.
fn is_pub(node: tree_sitter::Node, source: &str) -> bool {
    let mut cursor = node.walk();
    let result = node.children(&mut cursor)
        .any(|c| c.kind() == "visibility_modifier" && c.utf8_text(source.as_bytes()).unwrap_or("").starts_with("pub"));
    result
}
