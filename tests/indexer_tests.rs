#[cfg(test)]
mod tests {
    use asp_db::Database;
    use asp_core::Indexer;
    use std::path::PathBuf;

    #[test]
    fn test_indexer_creation() {
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, PathBuf::from("."));
        assert_eq!(indexer.project_root, PathBuf::from("."));
    }
}
