#[cfg(test)]
mod tests {
    use asp_db::Database;
    use asp_core::AspConfig;
    use asp_rules::RuleEngine;

    #[test]
    fn test_no_violations_on_empty_db() {
        let db = Database::open_in_memory().unwrap();
        let config = AspConfig::default();
        let engine = RuleEngine::new(&db, &config);
        let violations = engine.check_all().unwrap();
        assert!(violations.is_empty());
    }
}
