pub mod python;
pub mod typescript;
pub mod javascript;

use anyhow::Result;

pub use python::PythonParser;
pub use typescript::TypeScriptParser;
pub use javascript::JavaScriptParser;

#[derive(Debug, Clone, PartialEq)]
pub enum Language {
    Python,
    TypeScript,
    JavaScript,
}

pub struct ParseResult {
    pub imports: Vec<RawImport>,
    pub symbols: Vec<Symbol>,
    pub language: Language,
}

pub struct RawImport {
    pub source: String,
    pub line: usize,
}

pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub exported: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Function,
    Class,
    Variable,
    Export,
}

pub trait LanguageParser: Send + Sync {
    fn parse(&self, source: &[u8]) -> Result<ParseResult>;
    fn language(&self) -> Language;
}

pub fn parser_for_extension(ext: &str) -> Option<Box<dyn LanguageParser>> {
    match ext {
        "py" => Some(Box::new(PythonParser)),
        "ts" | "tsx" => Some(Box::new(TypeScriptParser)),
        "js" | "jsx" | "mjs" | "cjs" => Some(Box::new(JavaScriptParser)),
        _ => None,
    }
}
