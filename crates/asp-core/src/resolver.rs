use anyhow::Result;
use asp_db::Database;
use globset::{Glob, GlobSetBuilder};
use std::collections::{HashMap, HashSet};

pub struct Resolver<'a> {
    pub db: &'a Database,
}

impl<'a> Resolver<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn build_dependency_graph(&self) -> Result<HashMap<String, HashSet<String>>> {
        let mut graph: HashMap<String, HashSet<String>> = HashMap::new();

        let mut stmt = self.db.conn.prepare(
            "SELECT from_file, to_file FROM imports WHERE to_file IS NOT NULL"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (from, to) = row?;
            graph.entry(from).or_default().insert(to);
        }

        Ok(graph)
    }

    /// Match every indexed file against the component glob patterns from `architecture.toml`
    /// and update the `component` column in the DB accordingly.
    pub fn assign_components(&self, config: &crate::AspConfig) -> Result<()> {
        // Reset all assignments so removed/renamed components don't linger.
        self.db.conn.execute("UPDATE files SET component = NULL", [])?;

        for component in &config.component {
            let mut builder = GlobSetBuilder::new();
            for pattern in &component.paths {
                if let Ok(g) = Glob::new(pattern) {
                    builder.add(g);
                }
            }
            let Ok(globset) = builder.build() else { continue };

            // Fetch all file paths, then match and update in Rust (avoids SQL LIKE limitations).
            let mut stmt = self.db.conn.prepare("SELECT path FROM files")?;
            let paths: Vec<String> = stmt
                .query_map([], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();

            for path in paths {
                // Match against just the portion after any leading path separator so
                // patterns like "Sources/**" work regardless of absolute prefix.
                let match_target = std::path::Path::new(&path)
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");

                if globset.is_match(&match_target) || globset.is_match(&path) {
                    self.db.conn.execute(
                        "UPDATE files SET component = ?1 WHERE path = ?2",
                        rusqlite::params![component.name, path],
                    )?;
                }
            }
        }

        Ok(())
    }

    pub fn component_for_file(&self, file_path: &str) -> Result<Option<String>> {
        let result: Option<String> = self.db.conn
            .query_row(
                "SELECT component FROM files WHERE path = ?1",
                [file_path],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        Ok(result)
    }

    pub fn component_dependency_graph(&self) -> Result<HashMap<String, HashSet<String>>> {
        let mut graph: HashMap<String, HashSet<String>> = HashMap::new();

        let mut stmt = self.db.conn.prepare(
            r#"SELECT f1.component, f2.component
               FROM imports i
               JOIN files f1 ON i.from_file = f1.path
               JOIN files f2 ON i.to_file = f2.path
               WHERE f1.component IS NOT NULL AND f2.component IS NOT NULL
                 AND f1.component != f2.component"#
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (from, to) = row?;
            graph.entry(from).or_default().insert(to);
        }

        Ok(graph)
    }
}
