/// Integration tests that run ASP against a Python representation of its own architecture.
///
/// ASP only parses Python/JS/TS files, so we create Python source files that mirror the
/// actual Rust crate dependency structure of the ASP workspace, then verify the full
/// indexing → component assignment → rule checking pipeline produces correct results.
#[cfg(test)]
mod tests {
    use asp_core::{AspConfig, Indexer, Project, Resolver, Rule};
    use asp_db::Database;
    use asp_rules::{RuleEngine, Severity};
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    // ---------------------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------------------

    fn make_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("asp_self_test_{}", name));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn count_files(db: &Database) -> i64 {
        db.conn
            .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
            .unwrap_or(0)
    }

    fn count_imports(db: &Database) -> i64 {
        db.conn
            .query_row("SELECT COUNT(*) FROM imports", [], |r| r.get(0))
            .unwrap_or(0)
    }

    // ---------------------------------------------------------------------------
    // Test 1: Indexing the real ASP repo indexes Rust files
    //
    // ASP now parses Rust. Running it over its own workspace must complete without
    // error and index all .rs source files with their use declarations and symbols.
    // ---------------------------------------------------------------------------

    #[test]
    fn test_index_real_asp_repo_indexes_rust_files() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, repo_root.clone());

        indexer
            .index_directory(&repo_root)
            .expect("index_directory must not fail on the ASP workspace");

        // The workspace has many .rs files — all should be indexed.
        assert!(
            count_files(&indexer.db) > 0,
            "Rust files must be indexed now that the Rust parser is registered"
        );
        // Every .rs file uses at least one `use` declaration.
        assert!(
            count_imports(&indexer.db) > 0,
            "use declarations must be captured as imports"
        );
    }

    // ---------------------------------------------------------------------------
    // Test 2: Full pipeline – Python mirror of the ASP crate architecture
    //
    // We create one Python file per ASP crate with the import statements that
    // mirror the real Cargo.toml dependency edges, then verify:
    //   • every file is indexed exactly once
    //   • raw import statements are stored in the imports table
    //   • component_dependency_graph() reflects the correct inter-crate edges
    //   • no_cycles rule reports zero violations  (the real arch is a DAG)
    //   • no_unowned rule reports zero violations (every file gets a component)
    // ---------------------------------------------------------------------------

    /// Returns (temp_dir, absolute_path_for_each_module_file)
    fn create_asp_mirror(dir: &Path) -> HashMap<&'static str, String> {
        // Each entry: (crate_name, relative_file_path, python_source)
        // Import statements mirror the actual Cargo dependency edges.
        let modules: &[(&str, &str, &str)] = &[
            (
                "asp_cli",
                "asp_cli/main.py",
                "import asp_core\nimport asp_db\nimport asp_rules\nimport asp_llm\nimport asp_watcher\n",
            ),
            (
                "asp_core",
                "asp_core/lib.py",
                "import asp_db\nimport asp_parser\n",
            ),
            (
                "asp_rules",
                "asp_rules/engine.py",
                "import asp_core\nimport asp_db\n",
            ),
            (
                "asp_daemon",
                "asp_daemon/lib.py",
                "import asp_core\nimport asp_db\n",
            ),
            // Leaf crates – no imports
            ("asp_db",      "asp_db/lib.py",      "# base layer\n"),
            ("asp_parser",  "asp_parser/lib.py",  "# base layer\n"),
            ("asp_llm",     "asp_llm/client.py",  "# base layer\n"),
            ("asp_watcher", "asp_watcher/lib.py", "# base layer\n"),
        ];

        let mut paths: HashMap<&'static str, String> = HashMap::new();
        for &(crate_name, rel_path, source) in modules {
            write(dir, rel_path, source);
            // Use the same path format that `ignore::Walk` will store in the DB
            // (non-canonicalized, matching the dir prefix exactly).
            let abs = dir.join(rel_path);
            paths.insert(crate_name, abs.to_string_lossy().into_owned());
        }
        paths
    }

    #[test]
    fn test_asp_mirror_all_files_indexed() {
        let dir = make_temp_dir("all_files");
        let paths = create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());

        indexer.index_directory(&dir).unwrap();

        assert_eq!(
            count_files(&indexer.db),
            paths.len() as i64,
            "Every Python file must be indexed once"
        );
    }

    #[test]
    fn test_asp_mirror_raw_imports_captured() {
        let dir = make_temp_dir("raw_imports");
        create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());

        indexer.index_directory(&dir).unwrap();

        // The Python parser stores the full statement text as `to_raw`.
        // asp_cli/main.py has 5 import statements; check one of them.
        let found: bool = indexer
            .db
            .conn
            .query_row(
                "SELECT 1 FROM imports WHERE to_raw = 'import asp_core' LIMIT 1",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);

        assert!(found, "Raw import 'import asp_core' must be recorded");

        // Total raw imports: cli(5) + core(2) + rules(2) + daemon(2) = 11
        assert_eq!(
            count_imports(&indexer.db),
            11,
            "Exactly 11 raw import statements across all files"
        );
    }

    #[test]
    fn test_asp_mirror_component_dependency_graph() {
        let dir = make_temp_dir("comp_graph");
        let file_paths = create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());
        indexer.index_directory(&dir).unwrap();

        // 1. Assign every file to its component (crate name).
        for (crate_name, abs_path) in &file_paths {
            indexer
                .db
                .conn
                .execute(
                    "UPDATE files SET component = ?1 WHERE path = ?2",
                    rusqlite::params![crate_name, abs_path],
                )
                .unwrap();
        }

        // 2. Resolve `to_file` for each raw import statement.
        //    The `to_raw` text is the full statement, e.g. "import asp_db".
        //    We resolve it to the absolute path of the corresponding module file.
        let resolutions: &[(&str, &str)] = &[
            ("import asp_core",    "asp_core"),
            ("import asp_db",      "asp_db"),
            ("import asp_rules",   "asp_rules"),
            ("import asp_llm",     "asp_llm"),
            ("import asp_watcher", "asp_watcher"),
            ("import asp_parser",  "asp_parser"),
        ];
        for (raw, target_crate) in resolutions {
            let target_path = file_paths[target_crate].as_str();
            indexer
                .db
                .conn
                .execute(
                    "UPDATE imports SET to_file = ?1 WHERE to_raw = ?2",
                    rusqlite::params![target_path, raw],
                )
                .unwrap();
        }

        // 3. Build the component dependency graph and check edges.
        let resolver = Resolver::new(&indexer.db);
        let graph = resolver.component_dependency_graph().unwrap();

        // Expected edges derived from the Python file imports above:
        let expected_edges: &[(&str, &str)] = &[
            ("asp_cli",    "asp_core"),
            ("asp_cli",    "asp_db"),
            ("asp_cli",    "asp_rules"),
            ("asp_cli",    "asp_llm"),
            ("asp_cli",    "asp_watcher"),
            ("asp_core",   "asp_db"),
            ("asp_core",   "asp_parser"),
            ("asp_rules",  "asp_core"),
            ("asp_rules",  "asp_db"),
            ("asp_daemon", "asp_core"),
            ("asp_daemon", "asp_db"),
        ];

        for (from, to) in expected_edges {
            assert!(
                graph.get(*from).map_or(false, |tos| tos.contains(*to)),
                "Expected edge {} -> {} in component dependency graph",
                from,
                to
            );
        }

        // Leaf crates have no outgoing edges (they don't appear as `from` keys).
        for leaf in &["asp_db", "asp_parser", "asp_llm", "asp_watcher"] {
            assert!(
                !graph.contains_key(*leaf),
                "Leaf crate {} must not have outgoing component edges",
                leaf
            );
        }
    }

    #[test]
    fn test_asp_mirror_no_cycles_rule_passes() {
        let dir = make_temp_dir("no_cycles");
        let file_paths = create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());
        indexer.index_directory(&dir).unwrap();

        // Assign components.
        for (crate_name, abs_path) in &file_paths {
            indexer
                .db
                .conn
                .execute(
                    "UPDATE files SET component = ?1 WHERE path = ?2",
                    rusqlite::params![crate_name, abs_path],
                )
                .unwrap();
        }

        // Resolve imports.
        let resolutions: &[(&str, &str)] = &[
            ("import asp_core",    "asp_core"),
            ("import asp_db",      "asp_db"),
            ("import asp_rules",   "asp_rules"),
            ("import asp_llm",     "asp_llm"),
            ("import asp_watcher", "asp_watcher"),
            ("import asp_parser",  "asp_parser"),
        ];
        for (raw, target_crate) in resolutions {
            let target_path = file_paths[target_crate].as_str();
            indexer
                .db
                .conn
                .execute(
                    "UPDATE imports SET to_file = ?1 WHERE to_raw = ?2",
                    rusqlite::params![target_path, raw],
                )
                .unwrap();
        }

        let config = asp_config_with_rules(vec!["no_cycles"]);
        let engine = RuleEngine::new(&indexer.db, &config);
        let violations = engine.check_all().unwrap();

        assert!(
            violations.is_empty(),
            "ASP's own architecture is acyclic – no_cycles must report zero violations, got: {:?}",
            violations
        );
    }

    #[test]
    fn test_asp_mirror_no_unowned_rule_passes() {
        let dir = make_temp_dir("no_unowned");
        let file_paths = create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());
        indexer.index_directory(&dir).unwrap();

        // Assign every file to a component.
        for (crate_name, abs_path) in &file_paths {
            indexer
                .db
                .conn
                .execute(
                    "UPDATE files SET component = ?1 WHERE path = ?2",
                    rusqlite::params![crate_name, abs_path],
                )
                .unwrap();
        }

        let config = asp_config_with_rules(vec!["no_unowned"]);
        let engine = RuleEngine::new(&indexer.db, &config);
        let violations = engine.check_all().unwrap();

        assert!(
            violations.is_empty(),
            "Every file is assigned a component – no_unowned must report zero violations"
        );
    }

    #[test]
    fn test_asp_mirror_no_unowned_rule_detects_unassigned_file() {
        let dir = make_temp_dir("unowned_detect");
        let file_paths = create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());
        indexer.index_directory(&dir).unwrap();

        // Deliberately leave asp_cli/main.py unassigned (component = NULL).
        for (crate_name, abs_path) in &file_paths {
            if *crate_name != "asp_cli" {
                indexer
                    .db
                    .conn
                    .execute(
                        "UPDATE files SET component = ?1 WHERE path = ?2",
                        rusqlite::params![crate_name, abs_path],
                    )
                    .unwrap();
            }
        }

        let config = asp_config_with_rules(vec!["no_unowned"]);
        let engine = RuleEngine::new(&indexer.db, &config);
        let violations = engine.check_all().unwrap();

        assert_eq!(violations.len(), 1, "Exactly one unowned file expected");
        assert!(
            matches!(violations[0].severity, Severity::Warning),
            "no_unowned violations must have Warning severity"
        );
        assert!(
            violations[0].message.contains("asp_cli"),
            "Violation message must name the unowned file"
        );
    }

    #[test]
    fn test_asp_mirror_no_cycles_rule_detects_introduced_cycle() {
        let dir = make_temp_dir("cycle_detect");
        let file_paths = create_asp_mirror(&dir);
        let db = Database::open_in_memory().unwrap();
        let indexer = Indexer::new(db, dir.clone());
        indexer.index_directory(&dir).unwrap();

        // Assign components.
        for (crate_name, abs_path) in &file_paths {
            indexer
                .db
                .conn
                .execute(
                    "UPDATE files SET component = ?1 WHERE path = ?2",
                    rusqlite::params![crate_name, abs_path],
                )
                .unwrap();
        }

        // Resolve imports as before.
        let resolutions: &[(&str, &str)] = &[
            ("import asp_core",    "asp_core"),
            ("import asp_db",      "asp_db"),
            ("import asp_rules",   "asp_rules"),
            ("import asp_llm",     "asp_llm"),
            ("import asp_watcher", "asp_watcher"),
            ("import asp_parser",  "asp_parser"),
        ];
        for (raw, target_crate) in resolutions {
            let target_path = file_paths[target_crate].as_str();
            indexer
                .db
                .conn
                .execute(
                    "UPDATE imports SET to_file = ?1 WHERE to_raw = ?2",
                    rusqlite::params![target_path, raw],
                )
                .unwrap();
        }

        // Introduce a back-edge: asp_db → asp_cli  (creates a cycle)
        let db_path = &file_paths["asp_db"];
        let cli_path = &file_paths["asp_cli"];
        indexer
            .db
            .conn
            .execute(
                "INSERT OR IGNORE INTO imports (from_file, to_file, to_raw) VALUES (?1, ?2, 'import asp_cli')",
                rusqlite::params![db_path, cli_path],
            )
            .unwrap();

        let config = asp_config_with_rules(vec!["no_cycles"]);
        let engine = RuleEngine::new(&indexer.db, &config);
        let violations = engine.check_all().unwrap();

        assert!(
            !violations.is_empty(),
            "A cycle asp_cli→asp_db→asp_cli must be detected"
        );
        assert!(
            violations
                .iter()
                .any(|v| v.message.contains("asp_cli") || v.message.contains("asp_db")),
            "Violation message must name the components involved in the cycle"
        );
    }

    // ---------------------------------------------------------------------------
    // Helper: build an AspConfig with a given set of rule type names
    // ---------------------------------------------------------------------------

    fn asp_config_with_rules(rule_types: Vec<&str>) -> AspConfig {
        AspConfig {
            project: Project {
                name: "asp-self-test".to_string(),
                languages: vec!["python".to_string()],
                version: "1".to_string(),
            },
            component: vec![],
            rule: rule_types
                .into_iter()
                .enumerate()
                .map(|(i, t)| Rule {
                    name: format!("rule_{}", i),
                    rule_type: t.to_string(),
                    config: Default::default(),
                })
                .collect(),
        }
    }
}
