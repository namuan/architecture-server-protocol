#[cfg(test)]
mod tests {
    use asp_db::Database;
    use asp_core::Resolver;

    #[test]
    fn test_empty_graph() {
        let db = Database::open_in_memory().unwrap();
        let resolver = Resolver::new(&db);
        let graph = resolver.component_dependency_graph().unwrap();
        assert!(graph.is_empty());
    }
}
