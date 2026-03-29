use anyhow::Result;
use asp_db::Database;
use sha2::{Sha256, Digest};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};
use asp_parser::parser_for_extension;

pub struct Indexer {
    pub db: Database,
    pub project_root: PathBuf,
}

impl Indexer {
    pub fn new(db: Database, project_root: PathBuf) -> Self {
        Self { db, project_root }
    }

    pub fn index_file(&self, path: &Path) -> Result<()> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let Some(parser) = parser_for_extension(ext) else {
            return Ok(());
        };

        let content = std::fs::read(path)?;
        let hash = compute_hash(&content);
        let path_str = path.canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .into_owned();

        let existing_hash: Option<String> = self.db.conn
            .query_row(
                "SELECT hash FROM files WHERE path = ?1",
                [&path_str],
                |row| row.get(0),
            )
            .ok();

        if existing_hash.as_deref() == Some(&hash) {
            return Ok(());
        }

        info!("Indexing {}", path_str);

        let parse_result = parser.parse(&content)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.db.conn.execute(
            "INSERT OR REPLACE INTO files (path, hash, component, last_parsed) VALUES (?1, ?2, NULL, ?3)",
            rusqlite::params![path_str, hash, now],
        )?;

        self.db.conn.execute("DELETE FROM imports WHERE from_file = ?1", [&path_str])?;
        self.db.conn.execute("DELETE FROM symbols WHERE file = ?1", [&path_str])?;

        for import in &parse_result.imports {
            self.db.conn.execute(
                "INSERT OR IGNORE INTO imports (from_file, to_file, to_raw) VALUES (?1, NULL, ?2)",
                rusqlite::params![path_str, import.source],
            )?;
        }

        for symbol in &parse_result.symbols {
            let kind_str = format!("{:?}", symbol.kind);
            self.db.conn.execute(
                "INSERT OR IGNORE INTO symbols (file, name, kind, line, exported) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![path_str, symbol.name, kind_str, symbol.line as i64, symbol.exported as i64],
            )?;
        }

        Ok(())
    }

    pub fn index_directory(&self, dir: &Path) -> Result<()> {
        let walker = ignore::Walk::new(dir);
        for entry in walker {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Err(e) = self.index_file(path) {
                    warn!("Failed to index {}: {}", path.display(), e);
                }
            }
        }
        Ok(())
    }
}

pub fn compute_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}
