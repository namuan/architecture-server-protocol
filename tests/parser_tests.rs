#[cfg(test)]
mod tests {
    use asp_parser::{PythonParser, JavaScriptParser, TypeScriptParser, LanguageParser, Language};

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
}
