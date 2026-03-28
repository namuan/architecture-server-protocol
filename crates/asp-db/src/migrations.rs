use anyhow::Result;
use crate::Database;
use crate::schema::SCHEMA_SQL;

impl Database {
    pub fn run_migrations(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA_SQL)?;
        Ok(())
    }
}
