#[cfg(test)]
mod tests {
    use asp_parser::{PythonParser, JavaScriptParser, TypeScriptParser, RustParser, SwiftParser, LanguageParser, Language, SymbolKind};

    #[test]
    fn test_python_parse_empty() {
        let parser = PythonParser;
        let result = parser.parse(b"").unwrap();
        assert_eq!(result.language, Language::Python);
        assert!(result.imports.is_empty());
    }

    #[test]
    fn test_python_parse_import() {
        let parser = PythonParser;
        let result = parser.parse(b"import os\nimport sys").unwrap();
        assert_eq!(result.language, Language::Python);
        assert!(!result.imports.is_empty());
    }

    #[test]
    fn test_javascript_parse_empty() {
        let parser = JavaScriptParser;
        let result = parser.parse(b"").unwrap();
        assert_eq!(result.language, Language::JavaScript);
    }

    #[test]
    fn test_typescript_parse_empty() {
        let parser = TypeScriptParser;
        let result = parser.parse(b"").unwrap();
        assert_eq!(result.language, Language::TypeScript);
    }

    #[test]
    fn test_rust_parse_empty() {
        let result = RustParser.parse(b"").unwrap();
        assert_eq!(result.language, Language::Rust);
        assert!(result.imports.is_empty());
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn test_rust_parse_use_and_symbols() {
        let src = b"
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

pub struct Config { pub name: String }
pub enum Status { Active, Inactive }
pub trait Runner { fn run(&self); }
pub fn main() {}
fn private_helper() {}
";
        let result = RustParser.parse(src).unwrap();

        assert_eq!(result.language, Language::Rust);

        // imports
        assert_eq!(result.imports.len(), 2);
        assert!(result.imports[0].source.contains("std::collections::HashMap"));
        assert!(result.imports[1].source.contains("serde"));

        // symbols
        let fns: Vec<_> = result.symbols.iter().filter(|s| s.kind == SymbolKind::Function).collect();
        let types: Vec<_> = result.symbols.iter().filter(|s| s.kind == SymbolKind::Class).collect();

        assert!(fns.iter().any(|s| s.name == "main" && s.exported));
        assert!(fns.iter().any(|s| s.name == "private_helper" && !s.exported));
        assert!(types.iter().any(|s| s.name == "Config" && s.exported));
        assert!(types.iter().any(|s| s.name == "Status" && s.exported));
        assert!(types.iter().any(|s| s.name == "Runner" && s.exported));
    }

    #[test]
    fn test_swift_parse_empty() {
        let result = SwiftParser.parse(b"").unwrap();
        assert_eq!(result.language, Language::Swift);
        assert!(result.imports.is_empty());
        assert!(result.symbols.is_empty());
    }

    #[test]
    fn test_swift_parse_imports_and_symbols() {
        let src = b"
import Foundation
import UIKit

class ViewController: UIViewController {
    func viewDidLoad() {}
}

struct User {
    var name: String
}

protocol Describable {
    func describe() -> String
}

enum Direction { case north, south }
";
        let result = SwiftParser.parse(src).unwrap();

        assert_eq!(result.language, Language::Swift);

        assert_eq!(result.imports.len(), 2);
        assert!(result.imports[0].source.contains("Foundation"));
        assert!(result.imports[1].source.contains("UIKit"));

        let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"ViewController"));
        assert!(names.contains(&"User"));
        assert!(names.contains(&"Describable"));
        assert!(names.contains(&"Direction"));
    }
}
