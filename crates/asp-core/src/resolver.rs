use anyhow::Result;
use asp_db::Database;
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
